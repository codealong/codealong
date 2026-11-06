//! The schedules the harnesses keep themselves, outside Codealong's, for the Schedules view to list
//! beside its own, read only: what a Claude Code session scheduled inside its process (a `/loop`'s
//! next tick, a cron job; claude/wakeups.rs) and the Codex app's automations
//! (codex/automations.rs). Each harness's adapter says its own (`Adapter::own_schedules`); this
//! gathers them over every account that isn't turned off, when a window asks (calls.rs
//! `harness_schedules`), never as the board is made, since they're read from records and files.

use serde::Serialize;

use crate::config::Config;
use crate::model::{Board, Harness, Session};

/// A schedule a harness keeps, as the Schedules view lists it (`src/types.ts` `HarnessSchedule`).
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Kept {
    /// Unique among them: the harness, the account and its own id.
    pub key: String,
    pub harness: Harness,
    pub account: String,
    /// Its id where it's kept: a cron job's, an automation's.
    pub id: String,
    /// What the harness calls it: Loop, Cron job, Heartbeat, Automation.
    pub kind: String,
    pub name: String,
    pub prompt: String,
    pub when_words: String,
    /// When it runs next, where that can be told.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_at: Option<i64>,
    pub paused: bool,
    /// The session it wakes, or sends its runs to, where the board has it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// The folders its runs start sessions in, for one that starts its own: the projects it's in.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<String>,
    /// A link that opens it where it lives (`Adapter::open_link`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// What keeps it going, in a sentence: only while a process runs, or an app is open.
    pub lasts: String,
}

/// Every account's, the soonest to run first and the paused last.
pub fn list(cfg: &Config, board: &Board, now: i64) -> Vec<Kept> {
    let sessions: Vec<&Session> = board.lanes.iter().filter(|l| l.device.is_none()).flat_map(|l| l.sessions.iter()).filter(|s| s.visitor.is_none()).collect();
    let mut out: Vec<Kept> = cfg
        .accounts
        .iter()
        .filter(|a| !a.disabled)
        .filter_map(|a| crate::harness::of(a).map(|h| (a, h)))
        .flat_map(|(a, h)| {
            let mine: Vec<&Session> = sessions.iter().copied().filter(|s| s.account == a.name && s.harness == h.harness()).collect();
            h.own_schedules(a, &mine, now)
        })
        .collect();
    out.sort_by(|a, b| (a.paused, a.next_at.is_none(), a.next_at, &a.name).cmp(&(b.paused, b.next_at.is_none(), b.next_at, &b.name)));
    out
}

/// A prompt's first line, cut short, for a schedule named by what it's told.
pub fn named_by(prompt: &str) -> String {
    let line = prompt.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
    crate::util::one_line(line, 80)
}
