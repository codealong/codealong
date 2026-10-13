//! The worktrees and sessions a board shows, as every device sees them: the engine composes them
//! (board.rs), and a device's feed carries them to the devices it's linked with (feed.rs). Field
//! names serialize in camelCase to match `src/types.ts`. The board itself, and what only the Mac
//! app shows, are in the engine's model.rs.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Harness {
    Claude,
    Codex,
    Opencode,
    Antigravity,
    Cursor,
}

impl Harness {
    /// The harness an account's config names; anything unknown reads as Claude Code, which
    /// `config::check_accounts` never lets be saved.
    pub fn from_config(s: &str) -> Harness {
        match s {
            "codex" => Harness::Codex,
            "opencode" => Harness::Opencode,
            "antigravity" => Harness::Antigravity,
            "cursor" => Harness::Cursor,
            _ => Harness::Claude,
        }
    }

    /// Its name in config.toml and in a session's key (`from_config` reads it back).
    pub fn config(self) -> &'static str {
        match self {
            Harness::Claude => "claude",
            Harness::Codex => "codex",
            Harness::Opencode => "opencode",
            Harness::Antigravity => "antigravity",
            Harness::Cursor => "cursor",
        }
    }

    /// The harness as the UI names it.
    pub fn word(self) -> &'static str {
        match self {
            Harness::Claude => "Claude Code",
            Harness::Codex => "Codex",
            Harness::Opencode => "OpenCode",
            Harness::Antigravity => "Antigravity",
            Harness::Cursor => "Cursor",
        }
    }
}

/// Where a session runs: the configured terminal app, or the harness's own app.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Surface {
    Cli,
    App,
}

/// Session states, in priority order.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Needs,
    Error,
    Working,
    Unread,
    Idle,
    Stopped,
}

/// Board bays, in board order.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Bay {
    Needs,
    Working,
    Unread,
    Review,
    Idle,
    Merged,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Origin {
    Main,
    Script,
    Codex,
    /// In a `.claude/worktrees/` folder: made by `claude -w` or the Claude app.
    ClaudeApp,
    /// Inside a Claude session's scratchpad: made by that session.
    Scratchpad,
    Temp,
    Folder,
    Other,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum PrState {
    Draft,
    Open,
    Approved,
    Changes,
    Merged,
    Closed,
}

impl PrState {
    pub fn is_open(self) -> bool {
        matches!(self, PrState::Draft | PrState::Open | PrState::Approved | PrState::Changes)
    }
}

/// Whether GitHub would merge an open PR now, as its merge box says (`mergeStateStatus`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Merge {
    /// Nothing stands in the way: the reviews the repo requires, passing checks, no conflicts.
    Clean,
    /// The branch conflicts with its base.
    Conflicts,
    /// The repo wants the branch up to date with its base, and it's behind.
    Behind,
    /// Held back by a review or check the repo requires, a rule, or being a draft.
    Blocked,
    /// It could merge, but checks that aren't required didn't pass.
    Unstable,
    /// GitHub hasn't worked it out yet (asking starts that), or the PR is merged or closed.
    #[default]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// `claude:claude2:<id>`, `codex:codex:<id>`, `opencode:opencode:<id>`,
    /// `antigravity:antigravity:<id>` or `cursor:cursor:<id>`.
    pub key: String,
    pub id: String,
    pub short_id: String,
    pub harness: Harness,
    /// The account name as you type it: claude, claude2, codex, opencode, antigravity.
    pub account: String,
    pub surface: Surface,
    pub status: Status,
    /// A process is running this session right now.
    pub live: bool,
    pub pid: Option<u32>,
    pub title: String,
    /// Where it was started. Resuming and attaching happen here.
    pub cwd: String,
    /// Where its latest commands ran and edits landed, oldest first. It decides the session's
    /// lane (`board::session_seed`), so the UI doesn't need it.
    #[serde(skip)]
    pub work: Vec<String>,
    /// The end of its last reply, for Jev (jev.rs): where a question for you or a summary is.
    #[serde(skip)]
    pub reply_tail: Option<String>,
    /// The first prompt you gave it, for Jev.
    #[serde(skip)]
    pub first_prompt: Option<String>,
    /// The latest prompt you gave it, on one line, for Jev: what its last reply answers. None
    /// where its harness doesn't keep it (Antigravity, Cursor).
    #[serde(skip)]
    pub last_prompt: Option<String>,
    /// The first line of its last reply, for Jev: where a status line says how far the work got.
    #[serde(skip)]
    pub reply_head: Option<String>,
    /// The registry name, which Claude Code also puts in the terminal title.
    pub name: Option<String>,
    pub started_at: Option<i64>,
    /// When the current status began (turn start while Working).
    pub status_since: Option<i64>,
    pub last_activity: i64,
    /// When you last sent it a prompt, or when it started if its log doesn't say. Unlike its
    /// last activity this holds still while it works, so lists sort by it.
    pub last_prompt_at: i64,
    /// What the session is doing or asking. Backtick spans render as code.
    pub excerpt: Option<String>,
    pub resume_command: Option<String>,
    /// The Claude app is signed into this session's account (`~/.claude`), so it can open it.
    pub app_account: bool,
    /// The target that opens this session where it lives (focus, resume, Claude app, Codex,
    /// Antigravity).
    pub open_target: String,
    /// Why a session from another lane shows here: "made this worktree" or "worked here".
    /// Visitors don't count toward this lane's bay.
    pub visitor: Option<String>,
    /// This session's own one sentence, for rows in the agent bays.
    pub headline: String,
    pub detail: Option<String>,
    /// The branch it worked on: Claude's latest `gitBranch`, Codex's thread metadata.
    pub branch: Option<String>,
    /// Archived: from Codealong, in Codex, in OpenCode, or in the Claude app. Kept last in its
    /// lane and left out of the lane's status.
    pub archived: bool,
    /// A background job has no terminal tab; this opens it in a new one (`claude attach <job>`).
    pub attach_command: Option<String>,
    /// The session whose record holds this one's conversation until it writes its own: a
    /// Claude Code conversation moved to the background goes on in a job that writes nothing
    /// until its next prompt (`board::claude_moves`). Its panel reads again when this changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continues: Option<String>,
    /// The session it came from, when its harness or Codealong wrote that down: a fork of it,
    /// or started by it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<SessionOrigin>,
    /// Set when the session says it's busy but nothing has happened for a long while: it's stuck
    /// mid-turn, usually on a command waiting for input. When it went quiet.
    pub stalled_since: Option<i64>,
    /// Set on a Claude session whose turn ended with a command left running in the background
    /// (registry status `shell`): the command, or empty when it couldn't be read. A working one
    /// is waiting on it (a CI watch, a build), and the windows draw it with a clock instead of the
    /// spinner of a turn that's running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    /// How its turn ended, as Jev read it (insights.rs, when `[jev] sessions` is on) or as an
    /// agent tagged it (`codealong session tag`, tags.rs): `by` names the agent's session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insight: Option<SessionInsight>,
    /// Needs you only because its last reply asks you something (`board::apply_insight`): its
    /// turn ended and it sits at its prompt, so what you type is the answer. A session waiting on
    /// a permission prompt isn't one: what's typed there would answer the prompt.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub asks: bool,
    /// Codealong runs it itself, over the Agent Client Protocol (acp.rs): its panel sends to it
    /// and answers what it asks, and opening it shows the panel.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hosted: bool,
    /// Its turn is running and Codealong can stop it (`harness::stoppable`): a Stop sends its
    /// harness what its own Stop sends, or presses Escape in its terminal.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stoppable: bool,
    /// Its turn is running and what you send can reach the agent in that turn at once, rather
    /// than once the step it's on ends (`harness::steerable`): its reply box's Send now, Claude
    /// Code's send-now key and Codex's steer.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub steerable: bool,
    /// Snoozed from Codealong until then (`board::apply_snooze`): the floating bar and the mark
    /// leave it out of Needs you and Your turn until the time comes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snoozed_until: Option<i64>,
    /// Quieted from Codealong (`board::apply_quiet`): a turn it ends is idle rather than Your
    /// turn, so it waits on you only when it needs you (asks you something, waits on a call to
    /// allow, stops with an error). It stays quiet until you turn that off. The status already
    /// says it, so Macs before 2026-10-09, which don't send this, and devices that don't read it
    /// still leave its turns out.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub quiet: bool,
    /// Replies another session suggested you send it (`codealong session suggest`), shown above
    /// its box to send with a click, while they still answer where it stands
    /// (`board::suggestions_hold`). None from Macs before 2026-10-09.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestions: Option<Suggestions>,
    /// It worked in a reusable worktree before that worktree last took new work
    /// (`board::earlier`): kept in the lane's list after its current sessions, and out of the
    /// lane's status.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub earlier: bool,
    /// The Chrome tabs it drives through Claude in Chrome (chrometabs.rs), to bring back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chrome: Option<ChromeTabs>,
    /// Its last turn ended on its account's usage limit, and nothing has been said since.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<SessionLimit>,
    /// Its last turn ended because its harness couldn't reach its API, and nothing has been said
    /// since: the Mac slept or the network dropped, and the harness gave up retrying. None from
    /// Macs before 2026-10-08.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_lost: Option<ConnectionLost>,
    /// Its last turn was cut off: what ran it ended partway through (its terminal closed, the
    /// Mac restarted, it was killed), and nothing has gone on since. When the turn last did
    /// something. None from Macs before 2026-10-12.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut_off: Option<i64>,
    /// The model it's on, as its harness names it (`claude-opus-5-5`, `gpt-5.5`): the one its
    /// latest reply came from, or one it was switched to since. None where the harness doesn't
    /// say, and from Macs before 2026-10-06.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// How much of its model's context window its conversation fills now, as its latest call
    /// sent it. None where the harness doesn't say, before its first reply, and from Macs before
    /// 2026-10-12.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<ContextUse>,
    /// The worktree its harness made for it and holds (Claude Code's `--worktree` or
    /// `EnterWorktree`), which the harness removes as the session exits its own way, or asks
    /// whether to keep when anything in it would be lost. Mark as done ends it that way
    /// (`idle::ending`), so the harness cleans up after it. None where it has none, where its
    /// harness doesn't say, and from Macs before 2026-10-07.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub own_worktree: Option<String>,
    /// It works for an agent, not for you: started by another session's agent with `codealong
    /// start --helper`, or by an automatic review of a PR. A turn it ends is idle rather than
    /// Your turn, and while it doesn't need you it's out of its worktree's status, the bar and
    /// the mark, listed under what it helps (`Session::helping`). None from Macs before
    /// 2026-10-10, whose helpers show as sessions of their own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helper: Option<Helper>,
    /// The pages on the web it made or showed you (`Artifact`), the one it last published,
    /// updated or opened first. Empty where its harness makes none, and from Macs before
    /// 2026-10-12.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<Artifact>,
    /// The color its harness gives it, as the emblem's colors name it (Claude Code's `/color`):
    /// one of its emblem's two tones, beside the one Codealong gives it.
    #[serde(skip)]
    pub color: Option<String>,
    /// A small icon in a color beside its title, to tell it apart and remember it by
    /// (emblems.rs, when `[jev] emblems` is on). None until both are known, and from Macs before
    /// 2026-10-13.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emblem: Option<Emblem>,
}

/// A session's emblem: an icon that stands for what it works on, picked by Jev from the ones
/// `emblems::ICONS` lists, on a radial gradient of two tones: the color its harness gives it at
/// the middle, when it gives one, and a color of Codealong's own at the edge, which sets it apart
/// from the sessions beside it. Both are among `emblems::COLORS`: blue, teal, green, purple,
/// pink, red or indigo.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Emblem {
    /// Codealong's own color for it, never its harness's when it has one.
    pub color: String,
    /// The color its harness gives it (Claude Code's `/color`), when it gives one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness_color: Option<String>,
    /// The icon's name in the windows' icon set (Tabler's).
    pub icon: String,
}

/// A page on the web a session made or showed you, as its records say: an artifact Claude Code
/// published, made from a type or opened (`Adapter::artifact_made`). One a session published
/// again is one artifact, as it last stood.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub url: String,
    /// What the page is called.
    pub title: String,
    /// What the session last did with it, in a word or two: "Updated · version 9".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// When it last did, or 0 where its record doesn't say.
    pub at: i64,
}

/// What a helper session works for: the session whose agent started it, or the PR it reviews.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Helper {
    /// The session whose agent started it, by its key: the one its origin names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// The PR whose automatic review it is, by its key as reviews.json has it, which ignores case
    /// (`owner/name#7`, lowercased).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr: Option<String>,
    /// Its session is told when its turn ends or it waits on you (`codealong start --helper
    /// --report`), on the Mac they run on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub report: bool,
}

/// A session's id as lists show it: its first seven characters.
pub fn short_id(id: &str) -> String {
    id.chars().take(7).collect()
}

impl Session {
    /// A session's key: its harness, its account and its id there. This is the one place the
    /// key's form is written, so whatever makes a key from the three asks here.
    pub fn key_for(harness: Harness, account: &str, id: &str) -> String {
        format!("{}:{account}:{id}", harness.config())
    }

    /// Working only in that it waits on a command its turn left running in the background, which
    /// wakes it when it ends: counted as working, drawn waiting. `src/lanes.ts` `waitsInBackground`
    /// says the same for the windows.
    pub fn waits_in_background(&self) -> bool {
        self.status == Status::Working && self.background.is_some()
    }

    /// A helper that doesn't need you: out of its worktree's status, the bar, the mark and the
    /// queue, and listed under what it helps. One that needs you (a call to allow, a question, an
    /// error) waits on you as any session does, so its agent isn't left waiting on it unseen.
    /// `src/lanes.ts` `helping` says the same for the windows.
    pub fn helping(&self) -> bool {
        self.helper.is_some() && !matches!(self.status, Status::Needs | Status::Error)
    }

    /// A session of which only whose it is is known: in a terminal, idle, not running, with no
    /// title, folder or times. Whatever builds a session says what it knows and takes the rest
    /// from here (`..Session::new(…)`), so a new field's neutral value goes here and nowhere
    /// else. There's no `Default`: a session without a harness, an account and an id is no
    /// session.
    pub fn new(harness: Harness, account: &str, id: &str) -> Session {
        Session {
            key: Session::key_for(harness, account, id),
            id: id.to_string(),
            short_id: short_id(id),
            harness,
            account: account.to_string(),
            surface: Surface::Cli,
            status: Status::Idle,
            live: false,
            pid: None,
            title: String::new(),
            cwd: String::new(),
            work: Vec::new(),
            reply_tail: None,
            first_prompt: None,
            last_prompt: None,
            reply_head: None,
            name: None,
            started_at: None,
            status_since: None,
            last_activity: 0,
            last_prompt_at: 0,
            excerpt: None,
            resume_command: None,
            app_account: false,
            open_target: String::new(),
            visitor: None,
            headline: String::new(),
            detail: None,
            branch: None,
            archived: false,
            attach_command: None,
            continues: None,
            origin: None,
            stalled_since: None,
            background: None,
            insight: None,
            asks: false,
            hosted: false,
            stoppable: false,
            steerable: false,
            snoozed_until: None,
            quiet: false,
            suggestions: None,
            earlier: false,
            chrome: None,
            limit: None,
            connection_lost: None,
            cut_off: None,
            model: None,
            context: None,
            own_worktree: None,
            helper: None,
            artifacts: Vec::new(),
            color: None,
            emblem: None,
        }
    }
}

/// How full a session's context window is: the tokens its latest call sent, counted as its
/// harness counts them for its own context meter, and the window, where it's known.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextUse {
    pub used: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<u64>,
}

/// The usage limit a session's last turn ended on, as its harness recorded it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionLimit {
    /// The limit, as its account's `Meter` keys it (`session`, `week`, `model:<name>`), when
    /// the harness said which.
    pub meter: Option<String>,
    /// When the limit lifts, when the harness said.
    pub resets_at: Option<i64>,
    /// The account's extra usage was refused too (spent or turned off), so raising its spend
    /// limit would also let it go on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub spend_capped: bool,
    /// When it was refused.
    pub at: i64,
    /// Another account of its harness with room for it, to move it to (`board::limit_moves`).
    pub move_to: Option<String>,
    /// The model its last reply came from, which decides which of an account's limits it spends.
    #[serde(skip)]
    pub model: Option<String>,
}

/// A turn a lost connection ended, as its harness recorded it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionLost {
    /// When the turn ended.
    pub at: i64,
    /// Codealong on its Mac tells it to carry on once its API can be reached again (config.toml's
    /// `[sessions] continue_after_network_error`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub continues: bool,
}

/// Replies suggested for a session by another session that read it (a supervisor going over the
/// sessions that wait on you) through `codealong session suggest`. Each goes as it is, as if you'd
/// typed it, and nothing goes until you pick one; for questions it asks, choices of their own,
/// picked for you to answer with. Kept on the session's Mac (suggestions.rs) until something is
/// sent to the session or a turn of its starts.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Suggestions {
    /// The replies, best first; none where it picks among the choices of questions it asks.
    pub replies: Vec<String>,
    /// For questions the session asks you (`api::Pending::questions`): the labels of the choices
    /// picked for each, by its question, which its question box shows picked. Devices from before
    /// 2026-10-10 don't show them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub picks: BTreeMap<String, Vec<String>>,
    /// Why each question's pick, briefly, by its question, where the suggester said: its question
    /// box shows it with the pick. Devices from before 2026-10-11 don't show them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub why: BTreeMap<String, String>,
    /// Why, or what to weigh, in a sentence or two, when the suggester said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The session that suggested them, by its key on its own device, when one did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    /// That session's title when it suggested them, to name it by on any device.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_title: Option<String>,
    /// When they were suggested, by the clock of the session's Mac.
    pub at: i64,
    /// When the session was last sent a prompt as the suggester read it (its `last_prompt_at`):
    /// they answer the reply to that prompt, and nothing once it's been sent another. None from
    /// Macs before 2026-10-09, whose suggestions go by `at`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_at: Option<i64>,
}

/// Where a session came from: the session it was forked from or started by.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionOrigin {
    /// That session's key.
    pub key: String,
    pub how: OriginHow,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OriginHow {
    /// A copy of that session's conversation as far as it went, going on by itself: Claude
    /// Code's `/fork` and `/background` (a job's `forkParentSessionId`), the Claude app's Fork
    /// and `btw` (`forkedFromSessionId`), Codex's `fork` (`forked_from_id`), OpenCode's
    /// (`fork_session_id`), and Codealong's Fork.
    Fork,
    /// That session's agent started it: `codealong start` run from it, a thread the Codex app
    /// made for it (`create_thread`), or a task it offered in the Claude app (`spawn_task`).
    Started,
}

/// A session's tabs in Chrome, as its record last listed them.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChromeTabs {
    /// Chrome's ids for its tab group's tabs, the one it last acted on first; empty once
    /// they're closed.
    pub tabs: Vec<i64>,
    /// The page it last acted on, kept after its tabs close to open again.
    pub url: String,
    pub title: String,
    /// The Chrome profile they're in, as its picker names it, when Chrome has more than one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// That profile's folder (`Profile 12`), where the page opens again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_dir: Option<String>,
    /// When it last used Chrome.
    pub at: i64,
}

/// Jev's read of a session (insights.rs). Each part is there only when it was asked: how a turn
/// ended when one has, the command's risk while it waits on you.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionInsight {
    /// How likely it is that the reply ends asking you something.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asks_you: Option<f64>,
    /// How far the work got: landed, pushed, committed, uncommitted, blocked, none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub landing: Option<String>,
    /// How likely it is that what you asked for is done.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished: Option<f64>,
    /// How risky the command waiting on you looks, 0 (reads only) to 3 (destructive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk: Option<f64>,
    /// How likely it is that the command a quiet session runs waits for typing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waits_for_input: Option<f64>,
    /// What a command left running in the background does: polling, job, server or stalled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_p: Option<f64>,
    /// How sure Jev was of its least sure answer, 0 to 1, when it said. Devices from before
    /// 2026-10-12 don't know it, nor the three below.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// The session of the agent that tagged it (`codealong session tag`), by its key on its own
    /// device, when one did: its fields then stand over Jev's read of the same moment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    /// That session's title when it tagged it, to name it by on any device.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_title: Option<String>,
    /// Why, or what to weigh, in a sentence or two, when the agent said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Checks {
    pub passed: u32,
    pub failed: u32,
    pub pending: u32,
    pub skipped: u32,
    /// Names of failing checks.
    pub failing: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Pr {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: PrState,
    pub checks: Checks,
    pub merge: Merge,
    pub updated_at: i64,
    pub merged_at: Option<i64>,
    pub closed_at: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct GitInfo {
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// Uncommitted files, untracked included.
    pub dirty: u32,
    pub last_commit: Option<String>,
    pub last_commit_at: Option<i64>,
    /// Lines added and removed against the merge base with the default branch.
    pub added: Option<u32>,
    pub removed: Option<u32>,
    /// The default branch on the remote (`origin/main`), and how far HEAD has moved from it,
    /// whatever the branch tracks: commits only HEAD has (`base_ahead`) and commits only it has
    /// (`base_behind`). Other Macs' older versions don't send these.
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub base_ahead: Option<u32>,
    #[serde(default)]
    pub base_behind: Option<u32>,
    /// When the worktree last took new work, from its HEAD's reflog (`git::reset_at`): another
    /// branch checked out, or its branch reset or made afresh.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_at: Option<i64>,
    /// How many times it took new work, as far back as its reflog goes (90 days by default):
    /// one for a worktree made for one piece of work, many for one that's reused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_work_count: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub id: String,
    pub icon: String,
    pub label: String,
    pub detail: Option<String>,
    pub kbd: Option<Vec<String>>,
    pub disabled: bool,
    /// `session`, `lane` or `pr`.
    pub group: String,
}

/// The project of worktrees in none of a device's projects. Such a lane stays off the board
/// (`hidden`) unless it needs you; a device that has the repo in a project puts it there.
pub const ELSEWHERE: &str = "Elsewhere";

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Lane {
    /// The worktree path, which is also its identity.
    pub id: String,
    pub project: String,
    pub repo: String,
    /// `owner/name` on GitHub.
    pub repo_slug: Option<String>,
    /// What GitHub calls the repo now, when that isn't `repo_slug`: the repo or its owner was
    /// renamed, and the remote still has the old name, which GitHub redirects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_slug_now: Option<String>,
    pub path: String,
    pub callsign: String,
    pub origin: Origin,
    pub branch: Option<String>,
    /// Short commit when the HEAD is detached.
    pub detached: Option<String>,
    pub bay: Bay,
    pub glyph: Status,
    pub parked: bool,
    /// Elsewhere lanes stay off the board unless they need you.
    pub hidden: bool,
    /// One sentence; backtick spans render as code, `{ago:<ms>}` and `{for:<ms>}` as live times.
    pub headline: String,
    pub detail: Option<String>,
    pub sessions: Vec<Session>,
    pub pr: Option<Pr>,
    pub git: Option<GitInfo>,
    pub last_activity: i64,
    /// Jev reads every session here as finished, and nothing is running (`[jev] sessions`): a
    /// worktree that may be ready to clean up.
    #[serde(default)]
    pub looks_finished: bool,
    pub targets: Vec<Target>,
    pub default_target: Option<String>,
    /// The device it's on, when that's another of your devices (peers.rs); none for this
    /// device's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<DeviceTag>,
    /// The app it serves, to open in the browser: the URL set for it, or a page a server in its
    /// folder serves. Only this device's own, since the link only opens here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<Preview>,
    /// What it holds, for a worktree that takes one piece of work after another (reusable: its
    /// menu's Reuse for new work, or a main checkout left as it is). None for one kept for a
    /// single piece of work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reuse: Option<Reuse>,
    /// Not marked either way, but its branch has carried several merged PRs: Settings suggests
    /// making it reusable (`board::looks_reusable`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub looks_reusable: bool,
    /// Codealong's own chats (the engine's `config::chats_dir`): a session started here gets a
    /// folder of its own in it, for a question that belongs to none of your projects. It's on
    /// the board with no sessions too, hidden, so there's always one to start in.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub starts_chats: bool,
    /// Codealong's own source: the worktree holds the Codealong app's `src-tauri/tauri.conf.json`
    /// (the engine's improve.rs). Improve Codealong starts its sessions here, on every device.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub codealong: bool,
}

impl Lane {
    /// A worktree of which only where it is and whose it is are known: its folder, which is
    /// also its id, its project and its repo. Idle, with no branch, sessions, PR or git state,
    /// and nothing said of how it was made. Whatever builds a lane says what it knows and takes
    /// the rest from here (`..Lane::new(…)`), so a new field's neutral value goes here and
    /// nowhere else.
    pub fn new(path: &str, project: &str, repo: &str) -> Lane {
        Lane {
            id: path.to_string(),
            project: project.to_string(),
            repo: repo.to_string(),
            repo_slug: None,
            repo_slug_now: None,
            path: path.to_string(),
            callsign: String::new(),
            origin: Origin::Other,
            branch: None,
            detached: None,
            bay: Bay::Idle,
            glyph: Status::Idle,
            parked: false,
            hidden: false,
            headline: String::new(),
            detail: None,
            sessions: Vec::new(),
            pr: None,
            git: None,
            last_activity: 0,
            looks_finished: false,
            targets: Vec::new(),
            default_target: None,
            device: None,
            preview: None,
            reuse: None,
            looks_reusable: false,
            starts_chats: false,
            codealong: false,
        }
    }
}

/// What a reusable worktree holds (`board::reuse_of`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Reuse {
    /// No work: nothing uncommitted or unlanded, and no agent at work. New work can start here.
    Free,
    /// Work under way: uncommitted changes, commits that haven't landed, or an agent at work.
    Open,
    /// Its PR is open.
    Review,
    /// Its PR merged and nothing has changed since: new work can start once it's back on the
    /// default branch.
    Landed,
}

/// A worktree's app: where it opens, and the server behind it while one runs.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    /// The URL set for the worktree (`codealong preview set`, or its menu), or
    /// `http://localhost:<port>` for a page a server in its folder serves.
    pub url: String,
    /// The URL was set for it; false when it's a page found serving there.
    pub set: bool,
    /// The port given with the URL: which of the worktree's servers is the app.
    pub port: Option<u16>,
    /// The server that serves it, while one listens in the worktree's folder; none when the app
    /// isn't running.
    pub server: Option<PreviewServer>,
    /// The Chrome profile it opens in: the worktree's own, or else its project's
    /// (`chrome_profile` in config.toml). None opens it in the default browser.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chrome_profile: Option<String>,
    /// The profile was set for the worktree, not taken from its project.
    #[serde(default)]
    pub chrome_profile_set: bool,
}

/// A process listening on a TCP port, whose folder is in the worktree.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewServer {
    pub pid: u32,
    pub port: u16,
    /// The process's name, such as `node`.
    pub name: String,
    /// The page's `<title>`.
    pub title: Option<String>,
    /// When the process started.
    pub started_at: i64,
    /// The key of the session whose command started it.
    pub session: Option<String>,
}

/// Which of your other devices a lane is on, as the windows mark it: its name, its short id, the
/// color you gave it (or the one its key picks), and whether it's connected now.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceTag {
    pub name: String,
    pub short: String,
    /// One of the label colors: gray, red, pink, purple, indigo, blue, teal, green.
    pub color: String,
    pub online: bool,
    pub last_seen: Option<i64>,
}

/// How close a limit is: the usage endpoint's own word, or ours from the percentage.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Normal,
    Warning,
    Critical,
}

/// One usage limit: the 5-hour session, the week, or a per-model week such as Fable's.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Meter {
    /// `session`, `week`, or `model:<name>`.
    pub key: String,
    /// Short, for the bar: `5h`, `Week`, `Fable`.
    pub label: String,
    /// Long, for the tooltip: `5-hour session`, `Weekly, Fable only`.
    pub window: String,
    pub percent: f64,
    pub resets_at: Option<i64>,
    pub severity: Severity,
    /// Set on a per-model limit.
    pub model: Option<String>,
}

/// An account's plan limits, in config order on the board.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub account: String,
    pub harness: Harness,
    /// `Max 20x`, `Pro`.
    pub plan: Option<String>,
    pub meters: Vec<Meter>,
    /// When the numbers were read: fetched (Claude), or reported by the last turn (Codex).
    pub as_of: Option<i64>,
    /// Why there are no numbers, in words.
    pub error: Option<String>,
    /// The fix for `error` is signing in again.
    pub needs_sign_in: bool,
    /// True until the first read finishes.
    pub loading: bool,
    /// Its `preferred` setting: Auto starts it over accounts without one while it has room.
    #[serde(default)]
    pub preferred: bool,
    /// The usage endpoint asked us to slow down.
    #[serde(skip)]
    pub rate_limited: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn a_key_is_the_harness_the_account_and_the_id() {
        assert_eq!(Session::key_for(Harness::Claude, "claude2", "abc"), "claude:claude2:abc");
        assert_eq!(Session::key_for(Harness::Codex, "codex", "0199-ab"), "codex:codex:0199-ab");
        assert_eq!(Session::key_for(Harness::Opencode, "opencode", "ses_1"), "opencode:opencode:ses_1");
        assert_eq!(Session::key_for(Harness::Antigravity, "antigravity", "c1"), "antigravity:antigravity:c1");
        let s = Session::new(Harness::Claude, "claude2", "abc12345-0000");
        assert_eq!((s.key.as_str(), s.id.as_str(), s.short_id.as_str(), s.account.as_str()), ("claude:claude2:abc12345-0000", "abc12345-0000", "abc1234", "claude2"));
        // A lane's folder is its id.
        let l = Lane::new("/r/app", "Acme", "app");
        assert_eq!((l.id.as_str(), l.path.as_str(), l.project.as_str(), l.repo.as_str()), ("/r/app", "/r/app", "Acme", "app"));
    }

    // The phone app and Macs of other builds read these, so how they're written can't move: the
    // JSON below is what builds before the constructors sent.

    #[test]
    fn a_session_goes_between_devices_as_it_did() {
        // With everything said, it reads and is written back the same.
        let sent: Value = serde_json::from_str(
            r#"{
                "key": "claude:claude2:abc", "id": "abc", "shortId": "abc", "harness": "claude", "account": "claude2", "surface": "app",
                "status": "needs", "live": true, "pid": 42, "title": "Fix the login", "cwd": "/r/app", "name": "login", "startedAt": 1,
                "statusSince": 2, "lastActivity": 3, "lastPromptAt": 4, "excerpt": "Bash · `ls`", "resumeCommand": "claude --resume abc",
                "appAccount": true, "openTarget": "s|claude:claude2:abc|focus", "visitor": "worked here", "headline": "Needs you", "detail": "now",
                "branch": "login", "archived": true, "attachCommand": "claude attach j1", "continues": "claude:claude2:from",
                "origin": { "key": "claude:claude2:from", "how": "fork" }, "stalledSince": 5,
                "insight": { "asksYou": 0.75, "landing": "pushed" }, "asks": true, "hosted": true, "stoppable": true, "snoozedUntil": 6, "quiet": true,
                "suggestions": { "replies": ["Yes, push it", "Run the tests first"], "picks": { "Push it?": ["Yes"] }, "why": { "Push it?": "Its checks pass" }, "note": "It asks whether to push", "by": "claude:claude:sup", "byTitle": "Supervisor", "at": 10, "promptAt": 4 },
                "earlier": true,
                "chrome": { "tabs": [7, 8], "url": "https://example.com/", "title": "Example", "profile": "Work", "profileDir": "Profile 1", "at": 9 },
                "helper": { "parent": "claude:claude2:from", "pr": "acme/app#7" }
            }"#,
        )
        .unwrap();
        let s: Session = serde_json::from_value(sent.clone()).unwrap();
        assert_eq!(serde_json::to_value(&s).unwrap(), sent);
        assert_eq!((s.harness, s.surface, s.status, s.origin.as_ref().map(|o| o.how)), (Harness::Claude, Surface::App, Status::Needs, Some(OriginHow::Fork)));

        // An older build's, from before the fields added since: it reads as the constructor's
        // session with what it says, and a session with nothing more to say is written as this.
        let old = r#"{"key":"codex:codex:0199abcd-12","id":"0199abcd-12","shortId":"0199abc","harness":"codex","account":"codex","surface":"cli","status":"idle","live":false,"pid":null,"title":"","cwd":"","name":null,"startedAt":null,"statusSince":null,"lastActivity":0,"lastPromptAt":0,"excerpt":null,"resumeCommand":null,"appAccount":false,"openTarget":"","visitor":null,"headline":"","detail":null,"branch":null,"archived":false,"attachCommand":null,"stalledSince":null}"#;
        let read: Session = serde_json::from_str(old).unwrap();
        assert_eq!(serde_json::to_string(&read).unwrap(), old);
        assert_eq!(serde_json::to_string(&Session::new(Harness::Codex, "codex", "0199abcd-12")).unwrap(), old);
    }

    #[test]
    fn a_lane_goes_between_devices_as_it_did() {
        let sent: Value = serde_json::from_str(
            r#"{
                "id": "/r/app-fix", "project": "Acme", "repo": "app", "repoSlug": "acme/app-old", "repoSlugNow": "acme/app", "path": "/r/app-fix",
                "callsign": "fix", "origin": "claude-app", "branch": "fix", "detached": "abc1234", "bay": "review", "glyph": "unread", "parked": true,
                "hidden": true, "headline": "PR #7 is open", "detail": "2 checks", "sessions": [],
                "pr": {
                    "number": 7, "title": "Fix it", "url": "https://github.com/acme/app/pull/7", "state": "open",
                    "checks": { "passed": 1, "failed": 2, "pending": 3, "skipped": 4, "failing": ["lint"] }, "merge": "blocked",
                    "updatedAt": 1, "mergedAt": 2, "closedAt": 3
                },
                "git": {
                    "upstream": "origin/fix", "ahead": 1, "behind": 2, "dirty": 3, "lastCommit": "Fix it", "lastCommitAt": 4, "added": 5, "removed": 6,
                    "base": "origin/main", "baseAhead": 7, "baseBehind": 8, "resetAt": 9, "newWorkCount": 10
                },
                "lastActivity": 11, "looksFinished": true,
                "targets": [{ "id": "l|terminal", "icon": "terminal", "label": "Terminal", "detail": "fix", "kbd": ["⌘", "T"], "disabled": false, "group": "lane" }],
                "defaultTarget": "l|terminal",
                "device": { "name": "Mini", "short": "ab12cd34ef", "color": "teal", "online": true, "lastSeen": 12 },
                "preview": {
                    "url": "http://localhost:5173", "set": true, "port": 5173,
                    "server": { "pid": 13, "port": 5173, "name": "node", "title": "App", "startedAt": 14, "session": "claude:claude:abc" },
                    "chromeProfile": "Work", "chromeProfileSet": true
                },
                "reuse": "landed", "looksReusable": true
            }"#,
        )
        .unwrap();
        let l: Lane = serde_json::from_value(sent.clone()).unwrap();
        assert_eq!(serde_json::to_value(&l).unwrap(), sent);
        assert_eq!((l.origin, l.bay, l.glyph, l.reuse), (Origin::ClaudeApp, Bay::Review, Status::Unread, Some(Reuse::Landed)));

        // An older build's lane says nothing of looking finished; a lane with nothing more to
        // say is written with it.
        let old = r#"{"id":"/r/app","project":"Acme","repo":"app","repoSlug":null,"path":"/r/app","callsign":"","origin":"other","branch":null,"detached":null,"bay":"idle","glyph":"idle","parked":false,"hidden":false,"headline":"","detail":null,"sessions":[],"pr":null,"git":null,"lastActivity":0,"targets":[],"defaultTarget":null}"#;
        let now = old.replace(r#""lastActivity":0,"#, r#""lastActivity":0,"looksFinished":false,"#);
        let read: Lane = serde_json::from_str(old).unwrap();
        assert_eq!(serde_json::to_string(&read).unwrap(), now);
        assert_eq!(serde_json::to_string(&Lane::new("/r/app", "Acme", "app")).unwrap(), now);
    }
}
