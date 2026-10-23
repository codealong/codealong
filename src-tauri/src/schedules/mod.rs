//! Schedules: work Codealong starts, or a message it sends a session, when a time comes or
//! something happens. Each schedule says when (`When`): at times on the clock (rule.rs), when a
//! command it runs every so often prints something new (a watch), when something happens on the
//! board (a PR merges, a PR's checks fail, a todo is ready), or only when it's fired, by
//! `codealong schedule fire` from a script or Run now in the Schedules view. And what it does
//! (`Run`): start a new session each time; keep one session going, its first run starting it
//! and the rest sent to it (its thread); or send to a session that's already there. A new
//! session starts on an account, a routing group or Auto, picked again at each run, as a start
//! from the quick panel is.
//!
//! What's detected is detected without an agent: a time, a command's output, the board. An
//! agent runs only once something has happened, so a schedule that watches costs no usage while
//! nothing does. What a run was for (a watch's new lines, the PR that merged) goes to the agent
//! as a file it's pointed at, as data rather than instructions, never typed into its prompt.
//!
//! The schedules are kept in schedules.json next to the config, written by the Schedules view,
//! `codealong schedule` and an editor alike (store.rs `change`, under its lock), and read again
//! when it changes. What happened to each, its runs, the session it keeps going, what its watch
//! saw, is in schedule-runs.json, which only the scheduler writes, but for the runs asked for
//! (`Runs::fired`), which `codealong schedule fire` and Run now add to. The scheduler (runner.rs)
//! runs in the app: with the app closed nothing runs, and a time missed meanwhile runs once when
//! it's back, unless the schedule says to skip it (`CatchUp`).
//!
//! A schedule runs on this machine alone, or on your machines (`RunsOn`): on one of them, the one
//! it prefers while it's around and can, else the next, or on each of them. Every machine keeps a
//! copy of the ones they share, from the others' feeds (machines.rs).

pub mod machines;
pub mod rule;
pub mod runner;

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::config::{support_dir, Config};
use crate::model::{Board, PrState, Status};
use crate::start::Place;

use rule::Zone;

/// The schedules, as written.
pub fn path() -> PathBuf {
    support_dir().join("schedules.json")
}

/// What happened to each schedule, and the runs asked for.
pub fn runs_path() -> PathBuf {
    support_dir().join("schedule-runs.json")
}

/// A schedule's own folder: where its watch keeps what it likes between checks
/// (`CODEALONG_STATE_DIR`), and its notes between runs.
pub fn folder(id: &str) -> PathBuf {
    support_dir().join("schedules").join(id)
}

/// What schedules.json holds, for the error when it doesn't read as that.
const WHAT: &str = "a list of schedules";
const WHAT_RUNS: &str = "the schedules' runs";
/// How many runs of each schedule are kept.
const KEEP_RUNS: usize = 30;
/// How many ids of what a schedule has run for (a watch's lines, PRs, todos) are kept, so it
/// never runs twice for one.
const KEEP_SEEN: usize = 1000;
/// A watch is checked no more often than this.
pub const MIN_WATCH_MS: i64 = 60_000;
/// A start whose session hasn't shown by now (one an app or a terminal UI names itself) is
/// given up on.
pub const PENDING_MS: i64 = 30 * 60_000;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct File {
    pub schedules: Vec<Schedule>,
    /// Schedules your machines shared that were removed, or made one machine's, and when: the
    /// other machines drop their copies from before then too (machines.rs `merged`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub gone: Vec<codealong_peer::schedules::Gone>,
}

impl File {
    /// The schedule a name or id names: its id exactly, else its name, whatever its case.
    pub fn find(&self, name: &str) -> Option<&Schedule> {
        let name = name.trim();
        self.schedules.iter().find(|s| s.id == name).or_else(|| self.schedules.iter().find(|s| s.name.trim().eq_ignore_ascii_case(name)))
    }
}

/// A schedule, as schedules.json has it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Schedule {
    pub id: String,
    pub name: String,
    /// It doesn't run until it's resumed; Run now still runs it.
    pub paused: bool,
    pub when: When,
    /// A command run before each run, in the run's folder: the run goes ahead only when it
    /// succeeds and prints something, which goes to the agent with the rest. Empty for none.
    pub only_if: String,
    pub run: Run,
    /// The sessions it starts are quiet (board.rs `apply_quiet`): a turn they end is idle rather
    /// than Your turn, and they wait on you only when they need you.
    pub quiet: bool,
    /// What a run does while the last one's session is still at work or waiting on you.
    pub overlap: Overlap,
    /// What a time missed while the Mac slept or Codealong was closed does once it's back.
    pub catch_up: CatchUp,
    /// The most runs a day; none for no limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_day: Option<u32>,
    /// Each run is told of a notes file in the schedule's folder, to read first and add to, so
    /// a new session each time still knows what the last ones found.
    pub notes: bool,
    pub created_at: i64,
    pub updated_at: i64,
    /// The session whose agent made it with `codealong schedule add`; none when you did. What
    /// it sends goes as Codealong's, not as yours.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// Which of your machines run it.
    #[serde(skip_serializing_if = "RunsOn::is_this")]
    pub runs_on: RunsOn,
    /// When it was last resumed, kept through later changes: what passed before then isn't run,
    /// on every machine that has a copy (machines.rs `merged`), however late it hears.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resumed_at: Option<i64>,
    /// The repo its folder is a checkout of, for your other machines, which run it in a checkout of
    /// their own wherever that is (`place`): as the worktree picked says it, on the machine where
    /// its folder was picked (`checkout_for`), and passed on as it is by the others.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkout: Option<Checkout>,
    /// Its folder was picked where its worktree had no repo another machine could tell (none on
    /// GitHub, or on no branch): it goes by its folder alone, and no later save says a repo for it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub folder_only: bool,
}

/// A checkout of a repo, as a schedule's folder is one: the repo's main checkout, or a worktree of
/// it on a branch.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Checkout {
    /// On GitHub, `owner/name`.
    pub repo: String,
    /// A linked worktree's branch; none for the main checkout.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

impl Checkout {
    /// The checkout a worktree is: none for one with no GitHub repo, or a linked one on no branch,
    /// which no other machine could tell from another.
    pub fn of(lane: &crate::model::Lane) -> Option<Checkout> {
        let repo = lane.repo_slug_now.clone().or_else(|| lane.repo_slug.clone())?;
        match lane.origin {
            crate::model::Origin::Main => Some(Checkout { repo, branch: None }),
            _ => lane.branch.clone().map(|b| Checkout { repo, branch: Some(b) }),
        }
    }

    /// Whether a worktree is a checkout of `repo`, by the name it has now or the one its remote says
    /// (a repo renamed on GitHub).
    fn of_repo(lane: &crate::model::Lane, repo: &str) -> bool {
        [lane.repo_slug_now.as_deref(), lane.repo_slug.as_deref()].into_iter().flatten().any(|r| r.eq_ignore_ascii_case(repo))
    }

    /// Whether a worktree is this checkout.
    fn is(&self, lane: &crate::model::Lane) -> bool {
        Checkout::of_repo(lane, &self.repo) && match &self.branch {
            None => lane.origin == crate::model::Origin::Main,
            Some(b) => lane.origin != crate::model::Origin::Main && lane.branch.as_deref() == Some(b.as_str()),
        }
    }
}

/// Which of your machines run a schedule (`[devices]`, Settings › Devices).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RunsOn {
    /// This machine alone: the one it was made on, or made this machine's.
    #[default]
    This,
    /// One of them each time: the one it prefers (a device key) while it's around and can run it,
    /// else the next that can, first choices before backups (`[devices] automations`).
    One {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        prefer: String,
    },
    /// Each of them, on its own.
    Every,
}

impl RunsOn {
    pub fn is_this(&self) -> bool {
        *self == RunsOn::This
    }

    /// Your machines share it: each keeps a copy.
    pub fn shared(&self) -> bool {
        !self.is_this()
    }
}

/// When a schedule runs.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum When {
    /// At the times a rule names (rule.rs): `weekdays 9:00`, `every 2h`.
    Time { rule: String },
    /// When a command it runs `every` so often prints something it hasn't before.
    Watch {
        command: String,
        every: String,
        #[serde(default)]
        mode: WatchMode,
    },
    /// When something happens on the board.
    Event {
        event: Event,
        /// Only in this repo (`owner/name`); empty for any.
        #[serde(default)]
        repo: String,
        /// For a todo: only one with this label; empty for any.
        #[serde(default)]
        label: String,
        /// For a PR: only one of yours.
        #[serde(default)]
        mine: bool,
    },
    /// Only when it's fired: `codealong schedule fire` from a script, or Run now.
    Fired,
}

impl Default for When {
    fn default() -> When {
        When::Fired
    }
}

/// What a watch's output is: each line one thing, run for once (a list of deploys, messages),
/// or the whole of it one value, run for whenever it changes (the latest deploy's id).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum WatchMode {
    #[default]
    Lines,
    Value,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Event {
    /// A PR the board lists merges.
    PrMerged,
    /// An open PR's checks start failing at a commit.
    ChecksFailed,
    /// A todo is ready to start: not started, and waiting on nothing.
    TodoReady,
}

/// What a schedule does when it runs.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Run {
    pub to: To,
    /// For `To::Session`: the session it sends to, by key.
    pub session: String,
    /// Where a new session starts: a worktree, or a repo's checkout (`~/` for home); empty for a
    /// chat of its own, in no project.
    pub folder: String,
    /// In a new worktree its agent makes (`claude --worktree`), where the account's harness can.
    pub worktree: bool,
    /// An account, a routing group, or empty for Auto: picked again at each start.
    pub account: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// Where a new session runs; none for out of sight (the harness's background service, else
    /// Codealong, else a terminal behind your windows), as a helper starts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place: Option<Place>,
    pub prompt: String,
    /// For `To::Thread`: once its session started this long ago (`1d`, `12h`), the next run
    /// starts a new one, so a session kept going for weeks doesn't carry all of them. Empty
    /// for never.
    pub fresh_after: String,
}

/// Who a run goes to.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum To {
    /// A new session each time.
    #[default]
    New,
    /// The schedule's own session: the first run starts it, the rest are sent to it.
    Thread,
    /// A session that's already there (`Run::session`).
    Session,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Overlap {
    /// A run due while the last one's session works or waits on you is skipped; one for a
    /// session it sends to waits for its turn to end.
    #[default]
    Skip,
    /// Another session starts beside it.
    Allow,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum CatchUp {
    /// The latest time missed runs once, late, when the Mac or Codealong is back (within a week).
    #[default]
    Once,
    /// Missed times are skipped.
    Skip,
}

/// What schedule-runs.json holds.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Runs {
    /// Each schedule's, by its id.
    pub of: BTreeMap<String, Track>,
    /// Runs asked for and not yet taken up, oldest first.
    pub fired: Vec<Fired>,
}

/// A run asked for: by `codealong schedule fire`, or Run now.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Fired {
    /// The request's own id, to take it off the list once it's run.
    pub id: String,
    pub schedule: String,
    pub at: i64,
    /// What it's for, so one thing runs once however often it's fired (`--key`): a deploy's id.
    pub key: String,
    pub title: String,
    pub text: String,
    /// Asked for in the Schedules view: it runs even while the schedule is paused.
    pub by_hand: bool,
}

/// What happened to a schedule.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Track {
    /// When the scheduler first had it: what was already there then (PRs merged, todos ready,
    /// a watch's lines) isn't run for.
    pub armed_at: i64,
    /// The latest of its times that's been run, skipped or missed.
    pub last_due: Option<i64>,
    /// The latest of its times this machine took a run of, for a schedule your machines share:
    /// the others don't run it again.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub took: Option<i64>,
    /// The session it keeps going (`To::Thread`), and since when; for `To::Session`, the copy
    /// the session it sends to went on as (`follows` says which session that was).
    pub thread: Option<String>,
    pub thread_since: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follows: Option<String>,
    /// The session its latest new-session run started: while it works or waits on you, the next
    /// run is skipped (`Overlap::Skip`). Kept apart from `runs`, which only keeps the latest few.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started: Option<String>,
    /// How many runs went on which day (days since 1970 on this Mac's clock), for `per_day`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day: Option<RunDay>,
    /// A run waiting for the session it goes to to end its turn.
    pub queued: Option<Queued>,
    /// Its latest runs, oldest first.
    pub runs: Vec<RunRecord>,
    /// The ids of what it has run for, or saw there as it was armed.
    pub seen: Vec<String>,
    /// A watch's last output, in `WatchMode::Value`.
    pub value: Option<String>,
    /// A watch has had its first look, which only notes what's there.
    pub seeded: bool,
    /// How its watch's latest check went.
    pub check: Option<Check>,
    /// When its watch last looked and the command worked: what it's told as `CODEALONG_SINCE`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_ok: Option<i64>,
    /// The lines its watch's last good look printed, every one: a line is new only when it's
    /// neither here nor in `seen`, which keeps only the latest it ran for.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub looked: Vec<String>,
    /// When its watch's last look printed something other than the one before, for the machine
    /// that looks next (machines.rs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_at: Option<i64>,
    /// When it was last resumed: another machine's watch from before then doesn't count here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_at: Option<i64>,
    /// Its notes as your machines last had them, for a schedule they share.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<KeptNotes>,
}

/// A shared schedule's notes, as they were when this machine last read or wrote them.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct KeptNotes {
    pub text: String,
    /// When they were written, wherever that was.
    pub at: i64,
    /// The notes file's time here once they were read or written, so a change made here is told
    /// from one that came from another machine.
    pub mtime: i64,
}

/// The runs that went on one day.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(default)]
pub struct RunDay {
    pub day: i64,
    pub runs: u32,
}

/// A run waiting on its session.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Queued {
    pub at: i64,
    /// Why each run that's waiting came due.
    pub why: Vec<String>,
    /// The file each one's findings are in, as a prompt refers to it.
    pub files: Vec<String>,
    /// The todos they're for, which a new session is linked to and told of.
    pub todos: Vec<String>,
}

/// A watch's latest check.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Check {
    pub at: i64,
    pub ok: bool,
    /// Why it failed, or what it found.
    pub said: String,
    /// Checks failed in a row.
    pub failures: u32,
}

/// One run of a schedule, or one that didn't go.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct RunRecord {
    pub at: i64,
    /// The time on its rule it was for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due: Option<i64>,
    /// Why it ran, in a few words: "9:00", "2 new lines from its watch", "PR #12 merged".
    pub why: String,
    pub outcome: Outcome,
    /// What else there is to say: why it was skipped or failed, where it went.
    pub said: String,
    /// The session it started or sent to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// A start whose session isn't known yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending: Option<Pending>,
    /// The machine it ran on, when that's another of yours: as the Schedules view shows it, never
    /// kept.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    /// A new session started.
    #[default]
    Started,
    /// Sent to a session that was running.
    Sent,
    /// A session that had stopped went on with it.
    Resumed,
    /// Waiting for its session's turn to end.
    Queued,
    Skipped,
    /// Its time passed while the Mac slept or Codealong was closed.
    Missed,
    Failed,
}

/// A start whose session its harness names itself, looked for among the next sessions of its
/// account in its worktree.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Pending {
    pub lane: String,
    pub account: String,
    pub at: i64,
}

/// Something a run is for: a watch's line, a PR, a todo.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Found {
    pub id: String,
    pub title: String,
    pub url: String,
    pub text: String,
    /// The todo it is, which a new session is linked to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub todo: Option<String>,
}

/// A schedule as the Schedules view shows it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Item {
    #[serde(flatten)]
    pub schedule: Schedule,
    /// When it runs, in a few words.
    pub when_words: String,
    /// Why it can't run as it is, when it can't.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// When it runs next, for a time or a watch's check.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queued: Option<Queued>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check: Option<Check>,
    /// Its latest runs, the newest first.
    pub runs: Vec<RunRecord>,
    /// Runs asked for that haven't gone yet.
    pub asked: usize,
    /// Where it runs, for one your machines share: "On Mini", "Here, in Mini's place".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs_on_words: Option<String>,
    /// Why this machine can't run it, for one your machines share that another can.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub here: Option<String>,
    /// The folder it runs in here, when that's another than its own: this machine's checkout of its
    /// repo, which is somewhere else (`place`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs_in: Option<String>,
}

/// The schedules, or why schedules.json doesn't read. No file is no schedules.
pub fn load() -> Result<File> {
    Ok(crate::store::read(&path(), WHAT)?.unwrap_or_default())
}

/// What happened to them. A file that doesn't read is none, and the scheduler won't write over it.
pub fn load_runs() -> Result<Runs> {
    Ok(crate::store::read(&runs_path(), WHAT_RUNS)?.unwrap_or_default())
}

/// Changes the schedules under the file's lock: `f` says what it did, or why it didn't.
pub fn change<R>(f: impl FnOnce(&mut File) -> Result<R>) -> Result<R> {
    let mut out = None;
    crate::store::change(&path(), WHAT, |file: &mut File| {
        let r = f(file);
        let changed = r.is_ok();
        out = Some(r);
        changed
    })?;
    out.unwrap_or_else(|| bail!("Couldn't change the schedules."))
}

/// Changes what happened to them, under the runs file's lock.
pub fn change_runs<R>(f: impl FnOnce(&mut Runs) -> R) -> Result<R> {
    let mut out = None;
    crate::store::change(&runs_path(), WHAT_RUNS, |runs: &mut Runs| {
        out = Some(f(runs));
        true
    })?;
    out.ok_or_else(|| anyhow::anyhow!("Couldn't note the schedule's run."))
}

/// A new schedule's id.
pub fn new_id() -> String {
    let id = crate::start::new_session_id();
    format!("s-{}", &id[..8])
}

/// A run asked for's id.
fn fired_id() -> String {
    let id = crate::start::new_session_id();
    format!("f-{}", &id[..8])
}

/// A length of time as written: `90s`, `5m`, `2h`, `1d`.
pub fn duration_ms(text: &str) -> Option<i64> {
    let t = text.trim().to_lowercase();
    let split = t.find(|c: char| !c.is_ascii_digit())?;
    let (n, unit) = t.split_at(split);
    let n: i64 = n.parse().ok().filter(|&n| n > 0)?;
    let unit_ms = match unit.trim() {
        "s" | "sec" | "secs" | "second" | "seconds" => 1000,
        "m" | "min" | "mins" | "minute" | "minutes" => 60_000,
        "h" | "hr" | "hrs" | "hour" | "hours" => 3_600_000,
        "d" | "day" | "days" => crate::util::DAY_MS,
        _ => return None,
    };
    Some(n * unit_ms)
}

/// Adds a run to a schedule's record, keeping the latest few.
pub fn note_run(track: &mut Track, run: RunRecord) {
    track.runs.push(run);
    let extra = track.runs.len().saturating_sub(KEEP_RUNS);
    track.runs.drain(..extra);
}

/// Notes ids as seen, keeping the latest few, and never the ones given now: a watch whose output
/// holds more lines than are kept would otherwise forget one of them each look and run for it again.
pub fn note_seen(track: &mut Track, ids: impl IntoIterator<Item = String>) {
    let given: Vec<String> = ids.into_iter().collect();
    let mut had: std::collections::HashSet<String> = track.seen.iter().cloned().collect();
    for id in &given {
        if had.insert(id.clone()) {
            track.seen.push(id.clone());
        }
    }
    let given: std::collections::HashSet<String> = given.into_iter().collect();
    let keep = KEEP_SEEN.max(given.len());
    let mut extra = track.seen.len().saturating_sub(keep);
    track.seen.retain(|id| {
        let drop = extra > 0 && !given.contains(id);
        if drop {
            extra -= 1;
        }
        !drop
    });
}

/// Days since 1970 of a time's date on a clock.
pub fn day_of(ms: i64, zone: &dyn Zone) -> i64 {
    let c = zone.civil(ms);
    crate::util::days_from_civil(c.year, c.month, c.day)
}

/// Why a schedule can't be saved as it is: what each part must be on its own, whatever the
/// board has. `now` is for its time's rule, which must still run after it.
pub fn check(s: &Schedule, cfg: &Config, now: i64, zone: &dyn Zone) -> std::result::Result<(), String> {
    if s.name.trim().is_empty() {
        return Err("Give it a name.".into());
    }
    match &s.when {
        When::Time { rule } => {
            let parsed = rule::parse_checked(rule)?;
            if rule::next_after(&parsed, now, s.created_at, zone).is_none() {
                return Err("That time has passed.".into());
            }
        }
        When::Watch { command, every, .. } => {
            if command.trim().is_empty() {
                return Err("Say what command it runs to look.".into());
            }
            match duration_ms(every) {
                Some(ms) if ms >= MIN_WATCH_MS => {}
                Some(_) => return Err("A watch looks at most once a minute.".into()),
                None => return Err(format!("Couldn't read “{every}” as how often it looks: write “5m” or “1h”.")),
            }
        }
        When::Event { .. } | When::Fired => {}
    }
    let run = &s.run;
    match run.to {
        To::Session if run.session.trim().is_empty() => return Err("Pick the session it sends to.".into()),
        _ if run.prompt.trim().is_empty() => return Err("Say what it's told.".into()),
        _ => {}
    }
    if run.to != To::Session {
        let account = run.account.trim();
        if !account.is_empty() && account != "auto" && !crate::config::valid_reviewer(cfg, account) {
            return Err(format!("{account} isn't one of the accounts or routing groups."));
        }
        if let Some(m) = run.model.as_deref().filter(|m| !crate::start::valid_model(m)) {
            return Err(format!("{m} isn't a model name Codealong can start a session on."));
        }
    }
    if run.to == To::Thread && !run.fresh_after.trim().is_empty() && duration_ms(&run.fresh_after).is_none() {
        return Err(format!("Couldn't read “{}” as how long its session goes on: write “1d” or “12h”.", run.fresh_after));
    }
    if s.per_day == Some(0) {
        return Err("A limit of no runs a day would never run it; pause it instead.".into());
    }
    Ok(())
}

/// Why a schedule can't run on this board now, past what `check` says of it: its folder or its
/// session isn't there.
fn board_problem(s: &Schedule, board: &Board) -> Option<String> {
    let run = &s.run;
    match run.to {
        To::Session => board.session(run.session.trim()).is_none().then(|| "The session it sends to isn't on the board.".to_string()),
        _ if run.folder.trim().is_empty() => None,
        _ if place(s, board).is_some() => None,
        _ => Some(match &s.checkout {
            Some(c) => format!("There's no checkout of {}{} here.", c.repo, c.branch.as_deref().map(|b| format!(" on {b}")).unwrap_or_default()),
            None => format!("{} isn't a worktree Codealong knows.", run.folder.trim()),
        }),
    }
}

/// The worktree a schedule runs in on this machine: its folder, when that's a worktree here, else,
/// for one your machines share, this machine's checkout of its repo (`Schedule::checkout`): the
/// main checkout, or a worktree on its branch, the one used last when there are several. One that
/// runs here alone goes by its folder alone, as it was made. None for one in no folder, or with no
/// checkout here.
pub fn place<'a>(s: &Schedule, board: &'a Board) -> Option<&'a crate::model::Lane> {
    let folder = s.run.folder.trim();
    if folder.is_empty() {
        return None;
    }
    worktree(board, folder).or_else(|| {
        let c = s.checkout.as_ref().filter(|_| s.runs_on.shared())?;
        board.lanes.iter().filter(|l| c.is(l)).max_by_key(|l| l.last_activity)
    })
}

/// The worktree a schedule's folder names: that worktree itself, never one that holds the folder
/// (`start::lane_at` falls back to that), which on a machine without it would be your home folder,
/// or a repo's checkout, and a run would start there instead.
pub fn worktree<'a>(board: &'a Board, folder: &str) -> Option<&'a crate::model::Lane> {
    let dir = crate::util::expand(folder);
    let real = |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let want = real(&dir);
    board.lanes.iter().find(|l| std::path::Path::new(&l.path) == dir || std::path::Path::new(&l.id) == dir || real(std::path::Path::new(&l.path)) == want)
}

/// When a schedule's watch looks next: `every` after its last look, or now when it hasn't.
pub fn next_check(every: &str, track: Option<&Track>) -> Option<i64> {
    let every = duration_ms(every)?.max(MIN_WATCH_MS);
    // Its first look is as soon as the scheduler has it: a look an `every` later would take what
    // happened meanwhile for what was there already, and never run for it.
    let track = track.filter(|t| t.armed_at > 0)?;
    Some(track.check.as_ref().map_or(track.armed_at, |c| c.at + every))
}

/// The next time a schedule's rule names after the last it ran (or was armed), which may have
/// passed if it's due now. None when it has no times, or they're over.
pub fn next_due(s: &Schedule, track: Option<&Track>, now: i64, zone: &dyn Zone) -> Option<i64> {
    let When::Time { rule } = &s.when else { return None };
    let parsed = rule::parse(rule).ok()?;
    // From the last it ran, else from when it was made: one added while Codealong was closed, its
    // time passing before the scheduler first had it, still runs then (or is noted as missed).
    let made = Some(s.created_at).filter(|&c| c > 0).or_else(|| track.map(|t| t.armed_at).filter(|&a| a > 0));
    let since = track.and_then(|t| t.last_due).or(made).unwrap_or(now);
    rule::next_after(&parsed, since, s.created_at, zone)
}

/// When it runs, in words.
pub fn when_words(when: &When) -> String {
    match when {
        When::Time { rule } => rule::parse(rule).map(|r| rule::words(&r)).unwrap_or_else(|_| rule.trim().to_string()),
        When::Watch { command, every, mode } => {
            let what = match mode {
                WatchMode::Lines => "new lines",
                WatchMode::Value => "a change",
            };
            format!("When `{}` prints {what}, every {}", crate::util::one_line(command, 60), every.trim())
        }
        When::Event { event, repo, label, mine } => {
            let whose = if *mine { "a PR of yours" } else { "a PR" };
            let place = if repo.trim().is_empty() { String::new() } else { format!(" in {}", repo.trim()) };
            match event {
                Event::PrMerged => format!("When {whose} merges{place}"),
                Event::ChecksFailed => format!("When {whose}'s checks fail{place}"),
                Event::TodoReady if label.trim().is_empty() => "When a todo is ready".to_string(),
                Event::TodoReady => format!("When a todo labeled {} is ready", label.trim()),
            }
        }
        When::Fired => "When it's fired".to_string(),
    }
}

/// What a schedule does, in a few words: "New session in web on claude-work", "Sends to “Fix
/// the header”".
pub fn does_words(s: &Schedule, board: &Board) -> String {
    let run = &s.run;
    let place = || {
        let folder = run.folder.trim();
        let name = if folder.is_empty() { "a chat".to_string() } else { crate::util::file_name(&crate::util::expand(folder)) };
        let on = Some(run.account.trim()).filter(|a| !a.is_empty() && *a != "auto").unwrap_or("Auto");
        format!("in {name}{} on {on}", if run.worktree { ", a new worktree each time," } else { "" })
    };
    match run.to {
        To::New => format!("New session {}", place()),
        To::Thread => format!("Keeps one session going {}", place()),
        To::Session => format!("Sends to {}", board.session(run.session.trim()).map_or_else(|| run.session.trim().to_string(), |(_, s)| format!("“{}”", s.title))),
    }
}

/// A time on this Mac's clock, as the command prints it: `Tue 2026-10-21 9:00`.
pub fn clock_text(ms: i64, zone: &dyn Zone) -> String {
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let c = zone.civil(ms);
    let wday = (crate::util::days_from_civil(c.year, c.month, c.day) + 4).rem_euclid(7) as usize;
    format!("{} {}-{:02}-{:02} {}", DAYS[wday], c.year, c.month, c.day, rule::clock(c.hour * 60 + c.minute))
}

/// The schedules as the Schedules view shows them: one your machines share with where it runs,
/// and with their runs of it among its own.
pub fn items(file: &File, runs: &Runs, board: &Board, cfg: &Config, now: i64, zone: &dyn Zone) -> Vec<Item> {
    file.schedules
        .iter()
        .map(|s| {
            let track = runs.of.get(&s.id);
            // Checked as it was when saved: a time once that has since run isn't a problem.
            let saved = check(s, cfg, s.updated_at.max(s.created_at), zone).err();
            let (problem, here, runs_on_words) = match &s.runs_on {
                RunsOn::This => (saved.or_else(|| board_problem(s, board)), None, None),
                // One this machine can't run, the others still do: it's said of this machine alone.
                RunsOn::Every => match machines::able(s, board, cfg, zone).err() {
                    Some(here) if saved.is_none() && !board.machines.others.is_empty() => (None, Some(here), Some("On every machine".to_string())),
                    here => (saved.or(here), None, Some("On every machine".to_string())),
                },
                RunsOn::One { .. } => {
                    let here = machines::able(s, board, cfg, zone).err();
                    let line = machines::line(s, &board.machines, here.is_none(), now);
                    let words = machines::words(s, &line, &board.machines);
                    match line.is_empty() {
                        true => (here.or_else(|| Some("None of your machines can run it now.".to_string())), None, Some(words)),
                        false => (saved, here, Some(words)),
                    }
                }
            };
            let mut shown: Vec<RunRecord> = track.map(|t| t.runs.iter().rev().take(20).cloned().collect()).unwrap_or_default();
            if s.runs_on.shared() {
                shown.extend(machines::their_runs(&s.id, &board.machines));
                shown.sort_by(|a, b| b.at.cmp(&a.at));
                shown.truncate(20);
            }
            let next_at = match (&s.when, s.paused) {
                (_, true) => None,
                (When::Time { .. }, _) => next_due(s, track, now, zone),
                (When::Watch { every, .. }, _) => next_check(every, track),
                _ => None,
            };
            Item {
                schedule: s.clone(),
                when_words: when_words(&s.when),
                problem,
                next_at,
                thread: track.and_then(|t| t.thread.clone()),
                queued: track.and_then(|t| t.queued.clone()),
                check: track.and_then(|t| t.check.clone()),
                runs: shown,
                asked: runs.fired.iter().filter(|f| f.schedule == s.id).count(),
                runs_on_words,
                here,
                runs_in: place(s, board).map(|l| l.path.clone()).filter(|p| std::path::Path::new(p) != crate::util::expand(s.run.folder.trim())),
            }
        })
        .collect()
}

/// What on the board a schedule's event is for now: each PR merged, PR whose checks fail at its
/// commit, or ready todo, by an id of its own.
pub fn found_on(board: &Board, when: &When, since: i64) -> Vec<Found> {
    let When::Event { event, repo, label, mine } = when else { return Vec::new() };
    let repo = repo.trim();
    let in_repo = |slug: &str| repo.is_empty() || slug.eq_ignore_ascii_case(repo);
    match event {
        Event::PrMerged => board
            .prs
            .iter()
            .filter(|p| p.state == PrState::Merged && p.merged_at.is_some_and(|at| at > since) && in_repo(&p.repo_slug) && (!mine || p.mine))
            .map(|p| Found { id: format!("pr-merged:{}", p.key), title: format!("#{} {} merged in {}", p.number, p.title, p.repo_slug), url: p.url.clone(), text: String::new(), todo: None })
            .collect(),
        Event::ChecksFailed => board
            .prs
            .iter()
            .filter(|p| p.state.is_open() && p.checks.failed > 0 && in_repo(&p.repo_slug) && (!mine || p.mine))
            .map(|p| Found {
                id: format!("checks:{}@{}", p.key, p.head_oid.as_deref().unwrap_or(&p.head)),
                title: format!("#{} {} in {}: checks failing", p.number, p.title, p.repo_slug),
                url: p.url.clone(),
                text: if p.checks.failing.is_empty() { String::new() } else { format!("Failing: {}", p.checks.failing.join(", ")) },
                todo: None,
            })
            .collect(),
        Event::TodoReady => board
            .todos
            .iter()
            .filter(|t| t.status == crate::todos::TodoStatus::Todo && crate::todos::is_ready(t, &board.todos))
            // A todo keeps its labels' ids; the schedule may name one by its name.
            .filter(|t| label.trim().is_empty() || t.labels.iter().any(|id| id.eq_ignore_ascii_case(label.trim()) || board.labels.iter().any(|l| &l.id == id && l.name.trim().eq_ignore_ascii_case(label.trim()))))
            .map(|t| Found { id: format!("todo:{}", t.id), title: t.title.clone(), url: String::new(), text: String::new(), todo: Some(t.id.clone()) })
            .collect(),
    }
}

/// What a watch printed, in `WatchMode::Lines`: each line one thing. A line that's a JSON object
/// with an `id` is that thing (`title`, `url` and `text` too, where it has them); any other line
/// is itself, by its words.
pub fn found_in(output: &str) -> Vec<Found> {
    let mut out: Vec<Found> = Vec::new();
    for line in output.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let parsed = serde_json::from_str::<serde_json::Value>(line).ok().filter(|v| v.get("id").is_some_and(|id| !id.is_null()));
        let found = match parsed {
            Some(v) => {
                let text = |k: &str| v.get(k).map(|x| x.as_str().map(str::to_string).unwrap_or_else(|| x.to_string())).unwrap_or_default();
                let id = text("id");
                let title = Some(text("title")).filter(|t| !t.is_empty()).unwrap_or_else(|| id.clone());
                Found { id, title, url: text("url"), text: text("text"), todo: None }
            }
            None => Found { id: line_id(line), title: crate::util::one_line(line, 200), url: String::new(), text: line.to_string(), todo: None },
        };
        if !out.iter().any(|f| f.id == found.id) {
            out.push(found);
        }
    }
    out
}

/// A line's id: the line itself, or a hash of a long one.
fn line_id(line: &str) -> String {
    if line.len() <= 200 {
        return line.to_string();
    }
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(line.as_bytes());
    format!("sha256:{}", digest.iter().take(12).map(|b| format!("{b:02x}")).collect::<String>())
}

/// A change to the schedules, as the Schedules view, `codealong schedule` and the snapshot ask
/// for it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum Op {
    /// Adds a schedule, or replaces the one with its id.
    Save { schedule: Schedule },
    Remove { id: String },
    Pause { id: String, paused: bool },
    /// Runs it now, as its time coming would, paused or not and past the day's limit.
    Run { id: String },
    /// Asks for a run, as a script does when something happened: `key` says what it's for, so
    /// it runs once for it however often it's fired.
    Fire {
        id: String,
        #[serde(default)]
        key: String,
        #[serde(default)]
        title: String,
        #[serde(default)]
        text: String,
    },
}

/// The repo a schedule saved here is a checkout of, and whether it goes by its folder alone. A
/// folder picked now is what its worktree here is. One kept as it was keeps the repo it has,
/// refreshed only from a checkout of that same repo here (a branch it moved to), never from
/// another in that folder (a fork), so a save of something else on another machine doesn't send
/// its runs elsewhere; one saved before schedules said their repo is said now.
pub fn checkout_for(s: &Schedule, was: Option<&Schedule>, board: Option<&Board>) -> (Option<Checkout>, bool) {
    let folder = s.run.folder.trim();
    if folder.is_empty() {
        return (None, false);
    }
    let local = board.and_then(|b| worktree(b, folder));
    let here = || match local {
        Some(lane) => Checkout::of(lane).map_or((None, true), |c| (Some(c), false)),
        None => (None, false),
    };
    match was.filter(|w| w.run.folder.trim() == folder) {
        None => here(),
        Some(w) => match &w.checkout {
            Some(c) => match local.filter(|l| Checkout::of_repo(l, &c.repo)).and_then(Checkout::of) {
                Some(mine) => (Some(mine), false),
                None => (Some(c.clone()), false),
            },
            None if w.folder_only => (None, true),
            None => here(),
        },
    }
}

/// Makes a change to the schedules. `by` is the session whose agent asked, from the command: a
/// schedule it adds is its own. `board` says which worktree its folder is (`checkout_for`). What
/// the change came to, for its window or command to show.
pub fn apply_op(op: Op, by: Option<&str>, board: Option<&Board>, cfg: &Config, now: i64, zone: &dyn Zone) -> Result<serde_json::Value> {
    let found = |file: &File, id: &str| -> Result<String> {
        match file.find(id) {
            Some(s) => Ok(s.id.clone()),
            None => bail!("There's no schedule named {id}. `codealong schedule list` lists them."),
        }
    };
    let out = match op {
        Op::Save { mut schedule } => {
            schedule.name = schedule.name.trim().to_string();
            let saved = change(|file| {
                let old = file.schedules.iter().position(|s| !schedule.id.is_empty() && s.id == schedule.id);
                if file.schedules.iter().any(|s| s.id != schedule.id && s.name.eq_ignore_ascii_case(&schedule.name)) {
                    bail!("There's a schedule named {} already.", schedule.name);
                }
                match old.map(|i| &file.schedules[i]) {
                    Some(was) => {
                        schedule.created_at = was.created_at;
                        schedule.created_by = was.created_by.clone();
                        schedule.resumed_at = if was.paused && !schedule.paused { Some(now) } else { was.resumed_at };
                        // Made this machine's alone: the other machines drop their copies.
                        if was.runs_on.shared() && !schedule.runs_on.shared() {
                            file.gone.push(codealong_peer::schedules::Gone { id: schedule.id.clone(), at: now });
                        }
                    }
                    None => {
                        if schedule.id.is_empty() {
                            schedule.id = new_id();
                        }
                        schedule.created_at = now;
                        schedule.created_by = by.map(str::to_string);
                    }
                }
                schedule.updated_at = now;
                (schedule.checkout, schedule.folder_only) = checkout_for(&schedule, old.map(|i| &file.schedules[i]), board);
                if let Err(why) = check(&schedule, cfg, now, zone) {
                    bail!("{why}");
                }
                if schedule.runs_on.shared() {
                    file.gone.retain(|g| g.id != schedule.id);
                }
                match old {
                    Some(i) => file.schedules[i] = schedule.clone(),
                    None => file.schedules.push(schedule.clone()),
                }
                Ok(schedule)
            })?;
            serde_json::to_value(saved)?
        }
        Op::Remove { id } => {
            let removed = change(|file| {
                let id = found(file, &id)?;
                // Your other machines drop their copies of one they share.
                if file.schedules.iter().any(|s| s.id == id && s.runs_on.shared()) {
                    file.gone.push(codealong_peer::schedules::Gone { id: id.clone(), at: now });
                }
                file.schedules.retain(|s| s.id != id);
                Ok(id)
            })?;
            change_runs(|r| {
                r.of.remove(&removed);
                r.fired.retain(|f| f.schedule != removed);
            })?;
            serde_json::json!({ "removed": removed })
        }
        Op::Pause { id, paused } => {
            // Resuming notes where its times go on from before it's let run, so the scheduler,
            // reading between the two files, can't run a time that passed while it was paused;
            // `resumed_at` is the same time, so it isn't resumed again (runner.rs `Note::Resumed`).
            // One unpaused by a save goes by `resumed_at` alone.
            if !paused {
                let id = found(&load()?, &id)?;
                change_runs(|r| resumed(r.of.entry(id.clone()).or_default(), now))?;
            }
            let s = change(|file| {
                let id = found(file, &id)?;
                let s = file.schedules.iter_mut().find(|s| s.id == id).expect("found");
                if s.paused && !paused {
                    s.resumed_at = Some(now);
                }
                s.paused = paused;
                s.updated_at = now;
                Ok(s.clone())
            })?;
            serde_json::to_value(s)?
        }
        Op::Run { id } => {
            let id = found(&load()?, &id)?;
            let asked = Fired { id: fired_id(), schedule: id, at: now, by_hand: true, ..Fired::default() };
            change_runs(|r| r.fired.push(asked.clone()))?;
            serde_json::to_value(asked)?
        }
        Op::Fire { id, key, title, text } => {
            let id = found(&load()?, &id)?;
            let key = key.trim().to_string();
            let asked = Fired { id: fired_id(), schedule: id.clone(), at: now, key: key.clone(), title: title.trim().to_string(), text: text.trim().to_string(), by_hand: false };
            let ran = change_runs(|r| {
                let ran = !key.is_empty() && (r.of.get(&id).is_some_and(|t| t.seen.contains(&key)) || r.fired.iter().any(|f| f.schedule == id && f.key == key));
                if !ran {
                    r.fired.push(asked.clone());
                }
                ran
            })?;
            if ran {
                bail!("It has run for {key} already, or is about to.");
            }
            serde_json::to_value(asked)?
        }
    };
    runner::poke();
    Ok(out)
}

/// A schedule resumed: what passed while it was paused isn't run, though Codealong was closed
/// meanwhile and never saw it pass. Its times go on from now, it's armed again (what's on the
/// board now isn't new), and its watch's next look only notes what's there. A time it took a run
/// of since (a resume on another machine heard late) stays run; one it let pass, on a copy still
/// paused here, runs.
pub fn resumed(t: &mut Track, now: i64) {
    t.last_due = Some(t.took.map_or(now, |d| d.max(now)));
    t.armed_at = 0;
    t.seeded = false;
    t.reset_at = Some(now);
}

/// A schedule's time as the Schedules view previews it while it's written: in words, and its
/// next few times; or why it can't be read.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Times {
    pub words: String,
    pub next: Vec<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

pub fn times(text: &str, now: i64, zone: &dyn Zone) -> Times {
    match rule::parse_checked(text) {
        Ok(r) => {
            let mut next = Vec::new();
            let mut at = now;
            while next.len() < 3 {
                match rule::next_after(&r, at, now, zone) {
                    Some(t) => {
                        next.push(t);
                        at = t;
                    }
                    None => break,
                }
            }
            let problem = next.is_empty().then(|| "That time has passed.".to_string());
            Times { words: rule::words(&r), next, problem }
        }
        Err(why) => Times { problem: Some(why), ..Times::default() },
    }
}

/// Whether a session is busy with a run: at work, or waiting on you.
pub fn busy(board: &Board, key: &str) -> bool {
    board.session(key).is_some_and(|(_, s)| s.live && (s.status == Status::Working || s.status == Status::Needs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Checks, Merge, PrItem};

    fn pr(key: &str, state: PrState, merged_at: Option<i64>) -> PrItem {
        let (slug, number) = key.split_once('#').unwrap();
        PrItem {
            key: key.into(),
            repo_slug: slug.into(),
            repo: "web".into(),
            project: "Acme".into(),
            number: number.parse().unwrap(),
            title: "Add teams".into(),
            url: format!("https://github.com/{slug}/pull/{number}"),
            state,
            author: Some("ana".into()),
            mine: false,
            review_requested: false,
            requested_by: None,
            requested_at: None,
            reviewers_asked: Vec::new(),
            snoozed_until: None,
            nudge: false,
            nudge_by_default: false,
            primary: None,
            primary_by: None,
            head: "teams".into(),
            base: Some("main".into()),
            checks: Checks::default(),
            merge: Merge::Unknown,
            additions: 1,
            deletions: 0,
            changed_files: 1,
            created_at: 0,
            updated_at: 0,
            merged_at,
            closed_at: None,
            lanes: Vec::new(),
            sessions: Vec::new(),
            head_oid: None,
            authored_by: None,
            labels: Vec::new(),
            auto_review: None,
            last_review: None,
            stale_approval: None,
            turn: None,
            shared_review: Default::default(),
        }
    }

    #[test]
    fn reads_what_a_watch_printed() {
        let found = found_in("deploy 41 done\n\n{\"id\": 42, \"title\": \"Deploy 42\", \"url\": \"https://example.com/42\"}\ndeploy 41 done\n{\"title\": \"no id\"}\n");
        let ids: Vec<&str> = found.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, ["deploy 41 done", "42", "{\"title\": \"no id\"}"]);
        assert_eq!(found[1].title, "Deploy 42");
        assert_eq!(found[1].url, "https://example.com/42");
        let long = "x".repeat(300);
        assert!(found_in(&long)[0].id.starts_with("sha256:"));
    }

    #[test]
    fn finds_what_an_event_is_for() {
        let mut board = Board::default();
        board.prs = vec![pr("acme/web#1", PrState::Merged, Some(50)), pr("acme/web#2", PrState::Merged, Some(200)), pr("acme/api#3", PrState::Merged, Some(300)), pr("acme/web#4", PrState::Open, None)];
        board.prs[3].checks.failed = 1;
        board.prs[3].head_oid = Some("abc".into());
        let merged = When::Event { event: Event::PrMerged, repo: "acme/web".into(), label: String::new(), mine: false };
        let ids: Vec<String> = found_on(&board, &merged, 100).into_iter().map(|f| f.id).collect();
        assert_eq!(ids, ["pr-merged:acme/web#2"]);
        let failing = When::Event { event: Event::ChecksFailed, repo: String::new(), label: String::new(), mine: false };
        assert_eq!(found_on(&board, &failing, 0)[0].id, "checks:acme/web#4@abc");
        let mine = When::Event { event: Event::ChecksFailed, repo: String::new(), label: String::new(), mine: true };
        assert!(found_on(&board, &mine, 0).is_empty());
    }

    #[test]
    fn keeps_the_latest_runs_and_ids() {
        let mut t = Track::default();
        for i in 0..40 {
            note_run(&mut t, RunRecord { at: i, ..Default::default() });
        }
        assert_eq!(t.runs.len(), KEEP_RUNS);
        assert_eq!(t.runs[0].at, 10);
        note_seen(&mut t, (0..KEEP_SEEN + 5).map(|i| i.to_string()));
        note_seen(&mut t, ["7".to_string()]);
        assert_eq!(t.seen.len(), KEEP_SEEN);
        assert_eq!(t.seen[0], "5");
    }

    #[test]
    fn never_forgets_what_it_was_given() {
        let mut t = Track::default();
        let lines: Vec<String> = (0..KEEP_SEEN + 1).map(|i| format!("line {i}")).collect();
        note_seen(&mut t, lines.clone());
        note_seen(&mut t, lines.clone());
        assert!(lines.iter().all(|l| t.seen.contains(l)));
        // Fewer later: the oldest that aren't given go.
        note_seen(&mut t, ["new".to_string()]);
        assert_eq!(t.seen.len(), KEEP_SEEN);
        assert!(t.seen.contains(&"new".to_string()));
    }

    #[test]
    fn a_time_before_it_was_armed_still_runs() {
        let z = rule::tests::Fake::utc();
        let made = rule::tests::ms(&z, 2026, 10, 20, 8, 0);
        let s = Schedule { created_at: made, when: When::Time { rule: "once 2026-10-20 9:00".into() }, ..Default::default() };
        // Codealong opened at 9:30: the scheduler armed it then.
        let armed = rule::tests::ms(&z, 2026, 10, 20, 9, 30);
        let track = Track { armed_at: armed, ..Track::default() };
        assert_eq!(next_due(&s, Some(&track), armed, &z), Some(rule::tests::ms(&z, 2026, 10, 20, 9, 0)));
    }

    #[test]
    fn resuming_skips_what_passed_while_paused() {
        let mut t = Track { armed_at: 5, last_due: Some(10), seeded: true, ..Track::default() };
        resumed(&mut t, 500);
        assert_eq!((t.last_due, t.armed_at, t.seeded, t.reset_at), (Some(500), 0, false, Some(500)));
        // Heard late, after it ran a later time: that time stays run.
        let mut ran = Track { last_due: Some(900), took: Some(900), armed_at: 5, ..Track::default() };
        resumed(&mut ran, 845);
        assert_eq!((ran.last_due, ran.reset_at), (Some(900), Some(845)));
        // Heard late, after its copy here, still paused, let that time pass: it runs.
        let mut passed = Track { last_due: Some(900), took: Some(600), armed_at: 5, ..Track::default() };
        resumed(&mut passed, 845);
        assert_eq!(passed.last_due, Some(845));
    }

    #[test]
    fn a_watch_looks_first_as_soon_as_its_armed() {
        assert_eq!(next_check("1h", None), None);
        let armed = Track { armed_at: 1000, ..Track::default() };
        assert_eq!(next_check("1h", Some(&armed)), Some(1000));
        let looked = Track { check: Some(Check { at: 2000, ok: true, ..Check::default() }), ..armed };
        assert_eq!(next_check("1h", Some(&looked)), Some(2000 + 3_600_000));
    }

    #[test]
    fn reads_lengths_of_time() {
        assert_eq!(duration_ms("5m"), Some(300_000));
        assert_eq!(duration_ms("2 h"), Some(7_200_000));
        assert_eq!(duration_ms("1d"), Some(crate::util::DAY_MS));
        assert_eq!(duration_ms("90s"), Some(90_000));
        assert_eq!(duration_ms("soon"), None);
        assert_eq!(duration_ms("0m"), None);
    }

    #[test]
    fn says_what_a_schedule_needs() {
        let cfg = Config::default();
        let z = rule::tests::Fake::utc();
        let now = rule::tests::ms(&z, 2026, 10, 20, 12, 0);
        let mut s = Schedule { id: "s-1".into(), name: "Triage".into(), when: When::Time { rule: "weekdays 9:00".into() }, created_at: now, ..Default::default() };
        assert_eq!(check(&s, &cfg, now, &z), Err("Say what it's told.".into()));
        s.run.prompt = "Look at last night's CI.".into();
        assert_eq!(check(&s, &cfg, now, &z), Ok(()));
        s.run.account = "nobody".into();
        assert!(check(&s, &cfg, now, &z).unwrap_err().contains("isn't one of the accounts"));
        s.run.account = String::new();
        s.when = When::Time { rule: "once 2026-10-19 9:00".into() };
        assert_eq!(check(&s, &cfg, now, &z), Err("That time has passed.".into()));
        s.when = When::Watch { command: "gh run list".into(), every: "10s".into(), mode: WatchMode::Lines };
        assert!(check(&s, &cfg, now, &z).unwrap_err().contains("once a minute"));
        s.run.to = To::Session;
        assert_eq!(check(&s, &cfg, now, &z), Err("A watch looks at most once a minute.".into()));
        s.when = When::Fired;
        assert_eq!(check(&s, &cfg, now, &z), Err("Pick the session it sends to.".into()));
    }

    #[test]
    fn says_when_it_runs() {
        assert_eq!(when_words(&When::Time { rule: "weekdays 9:00".into() }), "Weekdays at 9:00");
        assert_eq!(when_words(&When::Event { event: Event::PrMerged, repo: "acme/web".into(), label: String::new(), mine: true }), "When a PR of yours merges in acme/web");
        assert_eq!(when_words(&When::Event { event: Event::TodoReady, repo: String::new(), label: "overnight".into(), mine: false }), "When a todo labeled overnight is ready");
        assert_eq!(when_words(&When::Watch { command: "gh api deployments".into(), every: "5m".into(), mode: WatchMode::Value }), "When `gh api deployments` prints a change, every 5m");
    }

    #[test]
    fn a_schedules_worktree_is_that_worktree_not_one_that_holds_it() {
        let home = crate::util::home().to_string_lossy().into_owned();
        let lane = |path: &str| crate::model::Lane { id: path.into(), path: path.into(), ..crate::fixtures::lane(path, Vec::new()) };
        let board = Board { lanes: vec![lane(&home), lane("/Users/me/projects/northwind")], ..Board::default() };
        assert!(worktree(&board, "/Users/me/projects/northwind").is_some());
        // Your home folder holds it, but it isn't a worktree here: a run would start in the wrong place.
        assert!(worktree(&board, "~/projects/atlas").is_none());
        let s = Schedule { run: Run { folder: "~/projects/atlas".into(), prompt: "Look".into(), ..Run::default() }, ..Schedule::default() };
        assert!(board_problem(&s, &board).is_some_and(|p| p.contains("isn't a worktree")));
    }

    #[test]
    fn another_machine_runs_it_in_its_own_checkout_of_the_repo() {
        let lane = |path: &str, slug: &str, origin: crate::model::Origin, branch: &str, active: i64| crate::model::Lane { id: path.into(), path: path.into(), repo_slug: Some(slug.into()), origin, branch: Some(branch.into()), last_activity: active, ..crate::fixtures::lane(path, Vec::new()) };
        use crate::model::Origin::{ClaudeApp, Main};
        let board = Board { lanes: vec![lane("/Users/me/code/infra", "acme/infra", Main, "main", 5), lane("/Users/me/old/infra", "acme/infra", Main, "main", 2), lane("/Users/me/code/infra/.claude/worktrees/fix", "acme/infra", ClaudeApp, "fix-alerts", 9)], ..Board::default() };
        let main = Some(Checkout { repo: "Acme/Infra".into(), branch: None });
        let s = |folder: &str, checkout: Option<Checkout>| Schedule { run: Run { folder: folder.into(), prompt: "Look".into(), ..Run::default() }, checkout, runs_on: RunsOn::Every, ..Schedule::default() };
        // Made where it's laid out the same: that folder.
        assert_eq!(place(&s("/Users/me/old/infra", main.clone()), &board).map(|l| l.path.as_str()), Some("/Users/me/old/infra"));
        // Laid out otherwise: the repo's main checkout here, the one used last.
        assert_eq!(place(&s("~/projects/infra", main.clone()), &board).map(|l| l.path.as_str()), Some("/Users/me/code/infra"));
        // A worktree on a branch: one on that branch, and never the main checkout instead.
        let fix = Some(Checkout { repo: "acme/infra".into(), branch: Some("fix-alerts".into()) });
        assert_eq!(place(&s("~/projects/infra-fix", fix), &board).map(|l| l.path.as_str()), Some("/Users/me/code/infra/.claude/worktrees/fix"));
        let other = s("~/projects/infra-x", Some(Checkout { repo: "acme/infra".into(), branch: Some("gone".into()) }));
        assert!(place(&other, &board).is_none());
        assert_eq!(board_problem(&other, &board).as_deref(), Some("There's no checkout of acme/infra on gone here."));
        // No repo said: the folder alone.
        assert!(place(&s("~/projects/infra", None), &board).is_none());
        // One that runs here alone goes by its folder alone, as it was made.
        let alone = Schedule { runs_on: RunsOn::This, ..s("~/projects/infra", main.clone()) };
        assert!(place(&alone, &board).is_none());
        // What a worktree is, said for another machine.
        assert_eq!(Checkout::of(&board.lanes[2]), Some(Checkout { repo: "acme/infra".into(), branch: Some("fix-alerts".into()) }));
        assert_eq!(Checkout::of(&crate::fixtures::lane("/tmp/x", Vec::new())), None, "no repo");
    }

    #[test]
    fn its_repo_is_said_where_its_folder_was_picked() {
        let path = "/Users/me/code/infra";
        let lane = |slug: Option<&str>| crate::model::Lane { id: path.into(), path: path.into(), repo_slug: slug.map(String::from), ..crate::fixtures::lane("/w", Vec::new()) };
        let board = |slug: Option<&str>| Board { lanes: vec![lane(slug)], ..Board::default() };
        let s = |folder: &str, checkout: Option<Checkout>, folder_only: bool| Schedule { run: Run { folder: folder.into(), ..Run::default() }, checkout, folder_only, ..Schedule::default() };
        let repo = |r: &str| Some(Checkout { repo: r.into(), branch: None });
        // Picked here: what its worktree is; one with no repo to tell goes by its folder alone.
        assert_eq!(checkout_for(&s(path, None, false), None, Some(&board(Some("acme/infra")))), (repo("acme/infra"), false));
        assert_eq!(checkout_for(&s(path, None, false), None, Some(&board(None))), (None, true));
        // Something else changed on a machine with a fork in that folder: the repo it has stays.
        let acme = s(path, repo("acme/infra"), false);
        assert_eq!(checkout_for(&acme, Some(&acme), Some(&board(Some("me/infra")))), (repo("acme/infra"), false));
        // ... and so on one whose worktree there has none.
        assert_eq!(checkout_for(&acme, Some(&acme), Some(&board(None))), (repo("acme/infra"), false));
        // Laid out otherwise there: the repo it came with.
        let came = s("~/projects/infra", repo("acme/infra"), false);
        assert_eq!(checkout_for(&came, Some(&came), Some(&board(Some("acme/infra")))), (repo("acme/infra"), false));
        // Folder alone, by choice: no save says a repo for it, wherever that is.
        let alone = s(path, None, true);
        assert_eq!(checkout_for(&alone, Some(&alone), Some(&board(Some("me/infra")))), (None, true));
        // Saved before schedules said their repo: said now, from its worktree here.
        let before = s(path, None, false);
        assert_eq!(checkout_for(&before, Some(&before), Some(&board(Some("acme/infra")))), (repo("acme/infra"), false));
        // Another folder picked: what that one is.
        assert_eq!(checkout_for(&s("~/projects/other", None, false), Some(&came), Some(&board(Some("acme/infra")))), (None, false));
        // Its repo renamed on GitHub: saved under its new name, which a machine whose remote has it
        // finds.
        let old = s(path, repo("acme/old"), false);
        let renamed = Board { lanes: vec![crate::model::Lane { repo_slug_now: Some("acme/new".into()), ..lane(Some("acme/old")) }], ..Board::default() };
        let (now, _) = checkout_for(&old, Some(&old), Some(&renamed));
        assert_eq!(now, repo("acme/new"));
        let there = Board { lanes: vec![crate::model::Lane { id: "/Users/me/src/new".into(), path: "/Users/me/src/new".into(), ..lane(Some("acme/new")) }], ..Board::default() };
        let shared = Schedule { checkout: now, runs_on: RunsOn::Every, ..s("~/projects/new", None, false) };
        assert_eq!(place(&shared, &there).map(|l| l.path.as_str()), Some("/Users/me/src/new"));
    }

    #[test]
    fn reads_a_file_written_by_hand() {
        let text = r#"{"schedules": [{"id": "s-1", "name": "Triage", "when": {"kind": "time", "rule": "weekdays 9:00"}, "run": {"prompt": "Look", "place": "background"}}]}"#;
        let file: File = serde_json::from_str(text).unwrap();
        let s = &file.schedules[0];
        assert_eq!(s.run.to, To::New);
        assert_eq!(s.run.place, Some(Place::Background));
        assert_eq!(s.overlap, Overlap::Skip);
        assert_eq!(file.find("triage").map(|s| s.id.as_str()), Some("s-1"));
        assert!(file.find("other").is_none());
    }
}
