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
/// Marks sessions unread (`keys`): each is Your turn again, there and everywhere, until it's seen
/// or prompted. Macs before 2026-10-08 refuse it as a request they don't know.
pub const MARK_UNREAD: &str = "mark-unread";
/// Snoozes sessions (`keys`) until `until`, or wakes them with none: each leaves Needs you and
/// Your turn, there and everywhere, until then. Macs before 2026-10-09 refuse it as a request they
/// don't know.
pub const SESSION_SNOOZE: &str = "session-snooze";
/// Quiets sessions (`keys`), or makes them heard again with `quiet` false: a quiet one's turns
/// don't wait on you, there and everywhere, and it waits on you only when it needs you. Macs
/// before 2026-10-09 refuse it as a request they don't know.
pub const SESSION_QUIET: &str = "session-quiet";
/// Makes helpers (`keys`, `Session::helper`) sessions of your own: listed, waiting on you and
/// counted as any of yours, there and everywhere, for good. Macs before 2026-10-10 refuse it as a
/// request they don't know.
pub const SESSION_OWN: &str = "session-own";
/// Pins sessions (`keys`) at the top of the pinned ones, or unpins them with `pinned` false, as
/// the Sessions view's Pin does there. Macs before 2026-10-13 refuse it as a request they don't
/// know.
pub const SESSION_PIN: &str = "session-pin";
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
/// Forks a session (`key`) into a copy that goes on without a window, on `prompt` when there's
/// one, answering `Started`. Macs before 2026-09-30 refuse it: their forks open in a terminal,
/// which needs someone at the Mac.
pub const SESSION_FORK: &str = "session-fork";
/// Goes on with a session that isn't running (`key`) where `resume` says, on `prompt` when
/// there's one, answering how it went as a send does (`Sent`). A `session-send` to one goes on
/// in a terminal. Macs before 2026-10-06 refuse it as a request they don't know.
pub const SESSION_RESUME: &str = "session-resume";
/// Switches a running session (`key`) to another of its account's models (`model`), for that
/// session alone, answering what happened in a sentence. Macs before 2026-10-06 refuse it as a
/// request they don't know.
pub const SESSION_MODEL: &str = "session-model";
/// Switches a running session (`key`) to another of its account's effort levels (`effort`), for
/// that session alone, answering what happened in a sentence. Macs before 2026-10-14 refuse it as
/// a request they don't know.
pub const SESSION_EFFORT: &str = "session-effort";
/// Done with a session (`key`): it ends as far as its Mac can end it without ending more than it
/// (its terminal tab closes, its background job stops), and is marked seen and archived,
/// answering `Done`. Work under way holds it unless `stop` ends that too. Macs before 2026-10-07
/// refuse it as a request they don't know; the Mac asking then marks it seen and archives it.
pub const SESSION_DONE: &str = "session-done";
/// Suggests replies for a session (`key`): its box shows them, to send with a click, until
/// something is sent to it or a turn of its starts. No `replies` takes them away. They're shown,
/// never sent by themselves, so another device may suggest them as it may name a session. Macs
/// before 2026-10-09 refuse it as a request they don't know.
pub const SESSION_SUGGEST: &str = "session-suggest";
/// Tags a session (`key`) with an agent's read of where it stands (`params::Tag`), which stands
/// over Jev's for the same moment: whether its last reply asks its user something, how far the
/// work got, whether it's finished, how risky what it waits on looks, what a command it left
/// running does. No field takes the tag away. It changes only how the session is listed, so
/// another device may tag it as it may name a session. Macs before 2026-10-12 refuse it as a
/// request they don't know.
pub const SESSION_TAG: &str = "session-tag";
/// A PR's description, reviews, comments and changed files (`repo`, `number`), as the device's
/// `gh` reads them from GitHub, answering `pr::Detail`. Macs before 2026-10-10 refuse it as a
/// request they don't know, as they do the other PR requests.
pub const PR_DETAIL: &str = "pr-detail";
/// What a PR's merge box offers you (`repo`, `number`), answering `pr::MergeBox`.
pub const PR_MERGE_BOX: &str = "pr-merge-box";
/// Reviews a PR as you on GitHub (`repo`, `number`, `verdict`, `body`): approves it, comments on
/// it, or asks for changes, submitting your draft review of it when there's one and it's the one
/// the asking device showed (`draft`), answering `pr::Reviewed`.
pub const PR_REVIEW: &str = "pr-review";
/// Does one thing the merge box does to a PR as you on GitHub (`repo`, `number`, `id`, `op`):
/// merges it, turns auto-merge on or off, queues it, updates its branch, takes it out of draft or
/// deletes its branch, answering `pr::Merged`.
pub const PR_MERGE: &str = "pr-merge";
/// Snoozes your review of PRs (`keys`, `owner/name#number`) until `until`, or wakes it with none:
/// out of what waits on you there and everywhere until then. Kept on the device asked.
pub const PR_SNOOZE: &str = "pr-snooze";
/// Keeps an image you'll send a session or start one with (`name`, `data`: its bytes in base64),
/// as a paste into the Mac's reply box does, answering `Kept`: how a prompt refers to it
/// (`![name](images/<hash>.<ext>)`), which the device asked turns into the agent's way to the
/// image as the prompt goes, whatever the harness. Macs before 2026-10-10 refuse it as a request
/// they don't know.
pub const IMAGE_KEEP: &str = "image-keep";
/// The biggest image `image-keep` takes, and `image-read` answers: in base64, in a request or an
/// answer, it stays well inside a line (`proto::MAX_LINE`).
pub const IMAGE_MAX_BYTES: usize = 20 * 1024 * 1024;
/// An image a prompt refers to (`path`, `images/<hash>.<ext>`), answering `Image`. Only images
/// Codealong kept are read.
pub const IMAGE_READ: &str = "image-read";
/// Removes a worktree (`key`, its lane's id) when nothing in it would be lost (the lane's
/// `cleanup`), through the repo's own teardown when it declares one, answering `Removal`; with
/// `dry_run`, only says whether it can go and what would run. It can't be undone, so another
/// device needs remote control. Macs before 2026-10-12 refuse it as a request they don't know.
pub const WORKTREE_REMOVE: &str = "worktree-remove";

/// What each request carries, as its makers write it (the engine's windows and command, and a
/// device that only watches, client.rs) and the device asked reads it (the engine's actions.rs).
/// `key` is the session's key as the device asked knows it: a window's own request leaves it
/// out, and the engine adds it when the request goes to another device. Every field reads as
/// empty when it's missing, as the requests of older devices may leave it out. Where makers
/// once wrote a request differently, the shape is the engine's windows', which every Mac has
/// read from other Macs (`answers: null`, `now: false`, which Macs before them ignore).
pub mod params {
    use serde::{Deserialize, Serialize};
    use serde_json::Value;

    use super::StartSpec;

    /// For a request about a session that carries nothing else: `session-pending`,
    /// `session-way`, `session-stream`, `session-cancel`.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Session {
        #[serde(skip_serializing_if = "String::is_empty")]
        pub key: String,
    }

    /// `session-page`: the newest page without `before`, else the one before that cursor.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Page {
        #[serde(skip_serializing_if = "String::is_empty")]
        pub key: String,
        pub before: Option<u64>,
    }

    /// `session-newer`: what was said after the cursor `after`.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Newer {
        #[serde(skip_serializing_if = "String::is_empty")]
        pub key: String,
        pub after: u64,
    }

    /// `session-answer`: allows or denies the call `id`, or with `answers` answers the
    /// questions it asks (keyed by each question's text). A deny with `message` tells it what to
    /// do instead, as Claude Code's own "No, and tell Claude what to do differently" does: Macs
    /// before 2026-10-05 deny it and drop the words.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Answer {
        #[serde(skip_serializing_if = "String::is_empty")]
        pub key: String,
        pub id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub answer: Option<super::Answer>,
        pub answers: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub message: Option<String>,
    }

    /// `session-send`: a prompt, into the turn the session is taking at once with `now`.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Send {
        #[serde(skip_serializing_if = "String::is_empty")]
        pub key: String,
        pub text: String,
        pub now: bool,
    }

    /// `session-model`: the model, as its account's list names it (`opus`, `haiku[1m]`).
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Model {
        #[serde(skip_serializing_if = "String::is_empty")]
        pub key: String,
        pub model: String,
    }

    /// `session-effort`: the level, as its account's list names it (`high`, `xhigh`).
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Effort {
        #[serde(skip_serializing_if = "String::is_empty")]
        pub key: String,
        pub effort: String,
    }

    /// `session-resume`: where it goes on, and the prompt it goes on with, or none to wait for one.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Resume {
        #[serde(skip_serializing_if = "String::is_empty")]
        pub key: String,
        pub resume: super::ResumeIn,
        pub prompt: String,
    }

    /// `session-archive`: archives, or unarchives with `archived` false.
    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
    #[serde(default)]
    pub struct Archive {
        #[serde(skip_serializing_if = "String::is_empty")]
        pub key: String,
        pub archived: bool,
    }

    impl Default for Archive {
        fn default() -> Archive {
            Archive { key: String::new(), archived: true }
        }
    }

    /// `session-done`: whether to end work under way too (a turn, or a command its turn left
    /// running in the background).
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Done {
        #[serde(skip_serializing_if = "String::is_empty")]
        pub key: String,
        pub stop: bool,
    }

    /// `session-rename`.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Rename {
        #[serde(skip_serializing_if = "String::is_empty")]
        pub key: String,
        pub name: String,
    }

    /// `session-suggest`: the replies, in place of any it had; none takes them away. `picks`
    /// are choices of the questions it asks to pick in place of replies, each by its label or as
    /// `<header>: <label>` (suggestions.rs `picked`), and `pending` the id of those questions as
    /// they were read (`Pending::id`): picks for others are turned down. `by` and `by_title` name the session that
    /// suggests them, as its own device knows it, and `prompt_at` is the session's last prompt as
    /// it read it: they're turned down when the session's been sent another since. Macs before
    /// 2026-10-09 ignore it, and Macs before 2026-10-10 take picks without replies for none.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default, rename_all = "camelCase")]
    pub struct Suggest {
        #[serde(skip_serializing_if = "String::is_empty")]
        pub key: String,
        pub replies: Vec<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pub picks: Vec<String>,
        /// Why each of `picks`, briefly, in their order: empty for a pick without one, and none
        /// at all for picks without any. Macs before 2026-10-11 ignore them.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pub why: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub pending: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub note: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub by: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub by_title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub prompt_at: Option<i64>,
    }

    /// `session-tag`: an agent's read of a session, each field as `SessionInsight` has it and
    /// left out where the agent didn't say (`asks_you` and `finished` 0 or 1, `risk` 0 to 3,
    /// `landing` and `background` by their names), in place of any tag it had; with every field
    /// left out, the tag is taken away. `by` and `by_title` name the tagging session as its own
    /// device knows it, `note` is why in a sentence or two, and `prompt_at` is the session's
    /// last prompt as the agent read it: the tag is turned down when the session's been sent
    /// another since, and goes once it is.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default, rename_all = "camelCase")]
    pub struct Tag {
        #[serde(skip_serializing_if = "String::is_empty")]
        pub key: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub asks_you: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub landing: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub finished: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub risk: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub background: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub note: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub by: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub by_title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub prompt_at: Option<i64>,
    }

    /// `session-fork`: the copy starts on `prompt`, or waits for one when it's empty.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Fork {
        #[serde(skip_serializing_if = "String::is_empty")]
        pub key: String,
        pub prompt: String,
    }

    /// Sessions by key: `mark-seen`, `mark-unread`, `session-watch`.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Keys {
        pub keys: Vec<String>,
    }

    /// `worktree-remove`: whether only to say what would happen.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Remove {
        #[serde(skip_serializing_if = "String::is_empty")]
        pub key: String,
        pub dry_run: bool,
    }

    /// `image-keep`: the image's name, and its bytes in base64.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct ImageKeep {
        pub name: String,
        pub data: String,
    }

    /// `image-read`: how a prompt refers to it, `images/<hash>.<ext>`.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct ImageRead {
        pub path: String,
    }

    /// A PR, by its repo (`owner/name`) and number: `pr-detail`, `pr-merge-box`.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Pr {
        pub repo: String,
        pub number: u64,
    }

    /// `pr-review`: what the review says, and what it writes (optional for an approval). `draft`
    /// is the stamp of the draft review the asking device showed (`pr::Draft::stamp`), none for
    /// none shown: a review submits your draft, so the device asked refuses one whose draft isn't
    /// the one shown, rather than publish what nobody read.
    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
    pub struct PrReview {
        pub repo: String,
        pub number: u64,
        pub verdict: crate::pr::Verdict,
        #[serde(default)]
        pub body: String,
        #[serde(default)]
        pub draft: Option<String>,
    }

    /// `pr-merge`: the PR's node id, as its merge box gave it, and what to do.
    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
    pub struct PrMerge {
        pub repo: String,
        pub number: u64,
        pub id: String,
        pub op: crate::pr::Op,
    }

    /// `session-snooze`: until when (ms since the epoch), or awake with none.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Snooze {
        pub keys: Vec<String>,
        pub until: Option<i64>,
    }

    /// `session-quiet`: quiet, or heard again.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Quiet {
        pub keys: Vec<String>,
        pub quiet: bool,
    }

    /// `session-pin`: pinned, or unpinned.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Pin {
        pub keys: Vec<String>,
        pub pinned: bool,
    }

    /// `session-own`: the helpers to make sessions of your own.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Own {
        pub keys: Vec<String>,
    }

    /// `search`: sessions whose prompts and replies have every word.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Search {
        pub words: Vec<String>,
    }

    /// `route`: where `text` goes among `project`'s worktrees (empty for all of them).
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Route {
        pub project: String,
        pub text: String,
    }

    /// `start`: a session in one of the device's worktrees.
    #[derive(Serialize, Deserialize, Clone, Debug)]
    pub struct Start {
        pub spec: StartSpec,
    }
}

/// Who may make a request of a device, as the device asked decides it (the engine's peers.rs).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reach {
    /// Any linked device: it only reads.
    Read,
    /// Any linked device: it changes how a session is listed there (seen, archived, named), not
    /// what an agent does.
    Organize,
    /// Only with remote control on at the device asked: it drives an agent (a prompt, an answer,
    /// a start, a stop), or acts as you on GitHub through the device's `gh` (a review, a merge).
    /// The device asked records each one in its devices.log.
    Control,
}

/// Every request, with who may make it. A request missing here is one a device doesn't know.
pub const REQUESTS: [(&str, Reach); 36] = [
    (SESSION_PAGE, Reach::Read),
    (SESSION_NEWER, Reach::Read),
    (SESSION_PENDING, Reach::Read),
    (SESSION_WAY, Reach::Read),
    (SESSION_STREAM, Reach::Read),
    (SESSION_WATCH, Reach::Read),
    (SEARCH, Reach::Read),
    (START_OPTIONS, Reach::Read),
    (ROUTE, Reach::Read),
    (PR_DETAIL, Reach::Read),
    (PR_MERGE_BOX, Reach::Read),
    (IMAGE_READ, Reach::Read),
    (MARK_SEEN, Reach::Organize),
    (MARK_UNREAD, Reach::Organize),
    (SESSION_SNOOZE, Reach::Organize),
    (SESSION_QUIET, Reach::Organize),
    (SESSION_PIN, Reach::Organize),
    (SESSION_OWN, Reach::Organize),
    (SESSION_ARCHIVE, Reach::Organize),
    (SESSION_RENAME, Reach::Organize),
    (SESSION_SUGGEST, Reach::Organize),
    (SESSION_TAG, Reach::Organize),
    (PR_SNOOZE, Reach::Organize),
    (SESSION_SEND, Reach::Control),
    (SESSION_ANSWER, Reach::Control),
    (SESSION_CANCEL, Reach::Control),
    (START, Reach::Control),
    (SESSION_FORK, Reach::Control),
    (SESSION_RESUME, Reach::Control),
    (SESSION_MODEL, Reach::Control),
    (SESSION_EFFORT, Reach::Control),
    (SESSION_DONE, Reach::Control),
    (PR_REVIEW, Reach::Control),
    (PR_MERGE, Reach::Control),
    (IMAGE_KEEP, Reach::Control),
    (WORKTREE_REMOVE, Reach::Control),
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
    /// (a call's output, by `id`), `codealong` (what Codealong sent by itself), `peer` (another
    /// session's message, whole) or `note` (compaction, a message held for you to allow…). A
    /// device that doesn't know a kind shows it as a note; Macs before 2026-10-14 sent another
    /// session's message as a note of its first line.
    pub kind: String,
    /// Prose for you, agent, peer and note; for a tool, a one-line label such as "Bash · `cargo test`".
    pub text: String,
    /// Who sent a `peer` message: the sending session's name, where its record says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
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
    /// The questions a call asks you (Claude Code's `AskUserQuestion`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub questions: Vec<Question>,
    /// Your answers to them, on the call's result: each question's text to what you picked or
    /// wrote, several where it took more.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub answers: std::collections::BTreeMap<String, Vec<String>>,
    /// What a call made or opened that has a page on the web (a Claude artifact), on the call's
    /// result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<Link>,
    /// Where its line starts in the record (or its message number); with `n`, unique in the session.
    pub cursor: u64,
    pub n: u32,
}

/// A page on the web that a tool call made or opened, which its line in the conversation links
/// to: an artifact Claude Code published.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub url: String,
    /// What the page is called.
    pub title: String,
    /// What the call did with it, in a word or two: "Published · version 3".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
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
    /// It can't be allowed just this once: its agent offers no answer that does (an Agent Client
    /// Protocol adapter needn't), so the windows offer no Allow, only Always where `always` says
    /// so. Devices that don't know it offer Allow, and it's refused.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_once: bool,
    /// It's a plan to approve (Claude Code's plan mode): `input` is the plan, in Markdown, to read
    /// as prose rather than as a command.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub plan: bool,
    /// It's another session's message the session holds for you (Claude Code): `input` is the
    /// message, to read as prose; Allow delivers it and Deny drops it, which its sender is told.
    /// Devices that don't know it show it as a call to allow, which comes to the same.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub message: bool,
    /// What `Answer::Always` adds, in a sentence, when Codealong can say: "Allows
    /// `Bash(npm test:*)` in this project, for you."
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub always_rule: Option<String>,
    /// Questions it asks you (Claude Code's `AskUserQuestion`): they're answered with your picks
    /// (the `answers` of `session-answer`), not allowed. Devices that don't know them offer Allow
    /// and Deny, and an Allow is refused.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub questions: Vec<Question>,
    /// What the session wrote with this request isn't in its conversation yet, only where it
    /// runs: its harness writes it once the request is answered (Claude Code, for the message a
    /// question comes in). The panel says where to read it meanwhile.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unwritten: bool,
    /// It's a command the session's agent asks to run in a terminal of yours (`codealong terminal
    /// run`): `input` is the command, which Allow runs in a new tab there, as you, and Deny
    /// refuses. It's asked every time, so there's no `always`, and only you allow it: at its Mac
    /// or from a phone, not from another Mac. Devices that don't know it show it as a call to
    /// allow, which comes to the same. None from Macs before 2026-10-12.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<TerminalRun>,
}

/// Where a command a session asks to run in your terminal would run (`Pending::terminal`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalRun {
    /// The folder it runs in, as the windows show a path (`~/…`).
    pub cwd: String,
    /// The terminal app its tab opens in (`Ghostty`).
    pub app: String,
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

/// An image kept for a prompt (`image-keep`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Kept {
    /// How a prompt refers to it: `![name](images/<hash>.<ext>)`, on a line of its own.
    pub reference: String,
    /// `images/<hash>.<ext>`, named by its bytes, so the same image twice is kept once.
    pub path: String,
}

/// An image a prompt refers to (`image-read`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Image {
    /// Its bytes, in base64.
    pub data: String,
    /// `image/png`, `image/jpeg`, `image/gif` or `image/webp`.
    pub mime: String,
}

/// What being done with a session came to (`session-done`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Done {
    /// It was marked seen and archived. False when work under way held it, which `message` says,
    /// and nothing was done: asked again with `stop`, that ends too.
    pub done: bool,
    /// One sentence: what ended, or what's under way.
    pub message: String,
    /// Something didn't go as asked (a terminal left open, say), worth saying even where the
    /// session leaving its list says it was done.
    #[serde(default)]
    pub noted: bool,
}

/// What removing a worktree came to (`worktree-remove`), or with `dry_run` would.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Removal {
    /// It's gone; with `dry_run`, it can go.
    pub removed: bool,
    /// One sentence: what was done, or why it stays.
    pub message: String,
    /// What ran, or would, in order: the repo's teardown or git's commands, then the harness's
    /// for a background job that held it. None for one that stays.
    #[serde(default)]
    pub steps: Vec<String>,
    /// It went, but not all as asked: the rest of the repo's teardown failed, or a job's record
    /// stayed. `message` says what, and it's worth showing where "removed" is said for several.
    #[serde(default)]
    pub noted: bool,
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
    /// Nothing takes a prompt from outside the Cursor app: it comes forward on the chat with it
    /// on the clipboard. Only a device that knows Cursor (`proto::CURSOR`) sees one of its chats
    /// to ask.
    Cursor,
}

// ---- starting a session (`start-options`, `start`) ----

/// Where a new session runs.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Place {
    /// A new terminal window running the account's command.
    Terminal,
    /// Started without anything coming forward: Claude Code's background service (`--bg`), with
    /// no terminal, where `attach` opens one later; for a harness with no background service of
    /// its own, a terminal opened behind your windows.
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
    /// For `Background`: the account's harness has no background service of its own, so the
    /// session starts in a terminal opened behind your windows.
    #[serde(default)]
    pub behind: bool,
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
    /// The models a start on its command line can ask for, as the harness takes them; none when
    /// it takes none, or from a device that lists only Claude Code's (`claude_models`).
    #[serde(default)]
    pub models: Vec<ModelOption>,
    /// The effort levels a start on its command line can ask for, lowest first.
    #[serde(default)]
    pub efforts: Vec<ModelOption>,
    /// Its command line makes a new worktree for a session when asked.
    #[serde(default)]
    pub worktree: bool,
    /// Of `places`, where one of its sessions that isn't running can go on (`ResumeIn`), in the
    /// same order. Missing from devices before 2026-10-06, which resume one only in a terminal.
    #[serde(default)]
    pub resumes: Vec<Place>,
    /// Its running sessions that Codealong types into can be switched to another of `models`
    /// (`session-model`); those Codealong runs can be, whatever their harness. Missing from Macs
    /// before 2026-10-06.
    #[serde(default)]
    pub switches_model: bool,
    /// Its running sessions that Codealong types into can be switched to another of `efforts`
    /// (`session-effort`); those Codealong runs can be where their agent offers the level.
    /// Missing from Macs before 2026-10-14.
    #[serde(default)]
    pub switches_effort: bool,
}

/// Where a session that isn't running goes on, and on what: a place its account starts sessions
/// in (`StartAccount::resumes`), with the model and effort where that place takes them, as a
/// start's are (`StartSpec`). None of them is the account's own.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResumeIn {
    pub place: Place,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
}

impl Default for ResumeIn {
    /// A new terminal, on the account's own model: how a session went on before there was a choice.
    fn default() -> ResumeIn {
        ResumeIn { place: Place::Terminal, model: None, effort: None }
    }
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
    /// The session that asked for it, by key: `codealong start` run by an agent. The new session
    /// is marked as started by it (`Session::origin`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// It's a helper of `from` (`codealong start --helper`): it works for that session's agent,
    /// not for you (`Session::helper`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub helper: bool,
    /// A helper whose `from` is told when its turn ends or it waits on its user
    /// (`codealong start --helper --report`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub report: bool,
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
                Item { at: 1, kind: "you".into(), text: "Fix the build".into(), from: None, id: None, input: None, output: None, failed: false, withdrawn: false, questions: Vec::new(), answers: Default::default(), link: None, cursor: 0, n: 0 },
                Item { at: 2, kind: "tool".into(), text: "Bash · `cargo test`".into(), from: None, id: Some("t1".into()), input: Some("cargo test".into()), output: Some("ok".into()), failed: false, withdrawn: false, questions: vec![Question { question: "Which?".into(), header: None, options: vec![Choice { label: "This".into(), description: None }], multi_select: false }], answers: [("Which?".to_string(), vec!["This".to_string()])].into(), link: Some(Link { url: "https://claude.ai/artifact/Vuaw".into(), title: "Dashboard".into(), note: None }), cursor: 120, n: 1 },
                Item { at: 3, kind: "peer".into(), text: "Which test fails?\n\nThe parser's, I think.".into(), from: Some("Review the parser".into()), id: None, input: None, output: None, failed: false, withdrawn: false, questions: Vec::new(), answers: Default::default(), link: None, cursor: 300, n: 0 },
            ],
            before: Some(0),
            after: 400,
        };
        let back: Page = serde_json::from_value(serde_json::to_value(&page).unwrap()).unwrap();
        assert_eq!(back, page);
        let json = serde_json::to_string(&page).unwrap();
        assert!(json.contains(r#""before":0"#) && !json.contains("\"input\":null") && !json.contains("\"from\":null"), "{json}");
        assert_eq!(serde_json::to_value(Answer::Allow).unwrap(), "allow");
    }

    /// Each request's params write what its makers wrote before they were typed, and read what
    /// any device sends, older ones included.
    #[test]
    fn requests_carry_what_they_always_did() {
        use serde_json::json;
        fn v(p: impl Serialize) -> serde_json::Value {
            serde_json::to_value(p).unwrap()
        }
        let key = || "claude:claude:abc".to_string();
        assert_eq!(v(&params::Session::default()), json!({}));
        assert_eq!(v(&params::Session { key: key() }), json!({ "key": "claude:claude:abc" }));
        assert_eq!(v(&params::Page { key: String::new(), before: None }), json!({ "before": null }));
        assert_eq!(v(&params::Page { key: key(), before: Some(120) }), json!({ "key": "claude:claude:abc", "before": 120 }));
        assert_eq!(v(&params::Newer { key: key(), after: 400 }), json!({ "key": "claude:claude:abc", "after": 400 }));
        let answer = params::Answer { key: String::new(), id: "t1".into(), answer: Some(Answer::Deny), answers: None, message: None };
        assert_eq!(v(&answer), json!({ "id": "t1", "answer": "deny", "answers": null }));
        let saying = params::Answer { message: Some("Explain it first".into()), ..answer.clone() };
        assert_eq!(v(&saying), json!({ "id": "t1", "answer": "deny", "answers": null, "message": "Explain it first" }));
        let picks = params::Answer { answers: Some(json!({ "Which?": "This" })), answer: Some(Answer::Allow), ..answer };
        assert_eq!(v(&picks), json!({ "id": "t1", "answer": "allow", "answers": { "Which?": "This" } }));
        assert_eq!(v(&params::Send { key: String::new(), text: "hi".into(), now: false }), json!({ "text": "hi", "now": false }));
        assert_eq!(v(&params::Archive { key: key(), archived: false }), json!({ "key": "claude:claude:abc", "archived": false }));
        assert_eq!(v(&params::Rename { key: String::new(), name: "Fix it".into() }), json!({ "name": "Fix it" }));
        assert_eq!(v(&params::Fork { key: String::new(), prompt: String::new() }), json!({ "prompt": "" }));
        let background = ResumeIn { place: Place::Background, model: Some("opus".into()), effort: None };
        assert_eq!(v(&params::Resume { key: key(), resume: background, prompt: String::new() }), json!({ "key": "claude:claude:abc", "resume": { "place": "background", "model": "opus", "effort": null }, "prompt": "" }));
        assert_eq!(serde_json::from_value::<params::Resume>(json!({ "resume": { "place": "codealong" } })).unwrap().resume, ResumeIn { place: Place::Codealong, model: None, effort: None });
        assert_eq!(v(&params::Model { key: String::new(), model: "haiku".into() }), json!({ "model": "haiku" }));
        assert_eq!(v(&params::Effort { key: String::new(), effort: "high".into() }), json!({ "effort": "high" }));
        // Without a place it goes on in a terminal, as a send to it does.
        assert_eq!(serde_json::from_value::<params::Resume>(json!({ "prompt": "go on" })).unwrap().resume, ResumeIn::default());
        assert_eq!(v(&params::Keys { keys: vec![key()] }), json!({ "keys": ["claude:claude:abc"] }));
        assert_eq!(v(&params::Search { words: vec!["webhook".into()] }), json!({ "words": ["webhook"] }));
        assert_eq!(v(&params::Route { project: "All projects".into(), text: "fix it".into() }), json!({ "project": "All projects", "text": "fix it" }));
        let spec = StartSpec { lane_id: "/r/app".into(), account: "claude".into(), place: Place::Terminal, prompt: String::new(), model: None, effort: None, worktree: false, todos: vec![], from: None, helper: false, report: false };
        assert_eq!(v(&params::Start { spec }), json!({ "spec": { "laneId": "/r/app", "account": "claude", "place": "terminal", "prompt": "", "model": null, "effort": null, "worktree": false, "todos": [] } }));
        // What an older device leaves out reads as empty, an archive as one.
        let read = |j: serde_json::Value| (serde_json::from_value::<params::Answer>(j.clone()).unwrap(), serde_json::from_value::<params::Archive>(j).unwrap());
        let (answer, archive) = read(json!({ "key": "claude:claude:abc" }));
        assert_eq!((answer.answer, answer.answers, archive.archived), (None, None, true));
        assert_eq!(serde_json::from_value::<params::Send>(json!({ "key": "k", "text": "hi" })).unwrap().now, false);
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
    /// How likely it is that the work belongs in none of the worktrees: a question about
    /// nothing in them, for a chat in no project. Macs of earlier builds don't send it.
    #[serde(default)]
    pub no_repo: f64,
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

