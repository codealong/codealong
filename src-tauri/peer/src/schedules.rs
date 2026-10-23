//! The schedules your machines share, as each machine tells the others of them in its feed
//! (`FeedMsg::schedules`): a schedule that runs on one of your machines, or on every one, rather
//! than on the machine it was made on. Each machine sends its copy of each, whether it can run it
//! there, and what it has run and found, so that every machine keeps a copy of its own (one that's
//! asleep or off misses nothing), one machine runs each time, and the one that runs it next goes on
//! from where the last left off: what a watch saw, what an event ran for, the notes runs keep.
//! The schedules that left the list, removed or made one machine's again, are listed as gone, so
//! every machine drops its copy too.
//!
//! The Mac app's engine reads and writes them (schedules/machines.rs); a phone runs none, and only
//! carries them, so the schedule itself and its runs go as the engine writes them, not as types of
//! this crate's.

use serde::{Deserialize, Serialize};

/// What a machine says of the schedules it shares.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Shared {
    pub list: Vec<SharedSchedule>,
    /// Schedules it shared that it removed, or made its own again, and when: a copy older than that
    /// goes, wherever it is.
    pub gone: Vec<Gone>,
}

impl Shared {
    pub fn get(&self, id: &str) -> Option<&SharedSchedule> {
        self.list.iter().find(|s| s.id == id)
    }
}

/// One schedule, as the machine that sends it has it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct SharedSchedule {
    pub id: String,
    /// The schedule as schedules.json writes it (the engine's schedules/mod.rs `Schedule`), with a
    /// folder under home written `~/…`, so it's the same folder on a machine whose home is elsewhere.
    pub schedule: serde_json::Value,
    /// It can run it there: its folder, its account and the session it sends to are there.
    pub able: bool,
    /// Why it can't, when it can't.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why_not: Option<String>,
    /// The latest of its times it took a run of: another machine doesn't run that time again.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub took: Option<i64>,
    /// The latest of its times it's done with, however: run, skipped, missed or paused through.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_due: Option<i64>,
    /// Its latest runs there, the newest first, as the engine writes them (`RunRecord`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<serde_json::Value>,
    /// How many runs it made on which day, for a limit a day that counts every machine's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub today: Option<Day>,
    /// The ids of what it ran for: a watch's lines, PRs, todos, what a script fired it for.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub seen: Vec<String>,
    /// What its watch saw when it last looked there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watch: Option<Watch>,
    /// The notes its runs keep, as they are there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<Notes>,
}

/// The runs a machine made on one day (days since 1970 on its clock).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Day {
    pub day: i64,
    pub runs: u32,
}

/// What a schedule's watch saw on a machine: the lines of its last good look, or its value, and
/// when that last changed, so the machine that looks next knows what's new.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Watch {
    pub looked: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub seeded: bool,
    pub changed_at: i64,
    /// When the schedule was armed there, which a look from before it was last resumed is older
    /// than: what passed while it was paused isn't run for.
    pub since: i64,
}

/// A schedule's notes, and when they were written.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Notes {
    pub text: String,
    pub at: i64,
}

/// A schedule that left a machine's shared list, and when.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Gone {
    pub id: String,
    pub at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_a_machine_sends_and_leaves_out_what_it_lacks() {
        let s: Shared = serde_json::from_value(serde_json::json!({ "list": [{ "id": "s-1", "schedule": { "id": "s-1", "name": "Triage" }, "able": true, "took": 900 }] })).unwrap();
        assert_eq!(s.get("s-1").map(|x| (x.able, x.took)), Some((true, Some(900))));
        assert!(s.gone.is_empty());
        let back = serde_json::to_value(&s).unwrap();
        assert!(back["list"][0].get("whyNot").is_none() && back["list"][0].get("runs").is_none());
    }
}
