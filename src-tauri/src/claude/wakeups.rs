//! What a session has scheduled inside itself to wake it later, by its transcript: a self-paced
//! `/loop`'s next tick (`ScheduleWakeup`), and a `/loop` at an interval or a check at a time
//! (`CronCreate`). The session's process holds them and nothing else fires them, so ending the
//! process loses them: `codealong hygiene` keeps it (`Adapter::wakes_at`).
//!
//! A resumed process brings back from the transcript what was still to come (2.1.310's code),
//! so what's pending here is what the session's process holds, whichever of its processes
//! scheduled it:
//!
//! - A `CronCreate` call whose result names its job (`toolUseResult: {id, recurring, durable}`,
//!   with `nextFireAt` from about 2.1.300), until a `CronDelete` of the job succeeds (its result
//!   `{id}`; a resume takes the call alone for one, but the running process keeps a job whose
//!   delete failed). Its `cron` is five fields on this Mac's clock. A one-shot job (`recurring:
//!   false`) fires once and goes, Claude Code moving one at :00 or :30 up to 90 seconds earlier.
//!   A recurring one lives 7 days after its call (`recurringMaxAgeMs`, "Auto-expires after 7
//!   days"): the running scheduler fires it once more after that and deletes it ("aged out …
//!   deleting after final fire"), while a resume after its 7 days leaves it out. Each fire is a
//!   `system` / `scheduled_task_fire` line naming the job (`taskId`). Claude Code doesn't bring
//!   back a durable one (`durable: true`) from the transcript: a process that starts reloads it
//!   from the project's `.claude/scheduled_tasks.json`, past its 7 days too, and it fires only
//!   while a process runs, so it counts here.
//! - The latest `ScheduleWakeup` call's result (`scheduledFor`, ms), which takes the place of any
//!   before it; `{stop: true}` cancels it (`stopped: true`), and a `scheduled_task_fire` line
//!   with `taskKind: "loop"` says it fired. A resumed process arms it again, and tells the
//!   session it won't fire when its time passed while none ran.
//! - A compaction carries the jobs and the wakeup in an `attachment` of type
//!   `session_cron_carry` (`tasks: [{id, cron, prompt, createdAt, recurring}]`, `ended` ids,
//!   `wakeup: {scheduledFor}`), for a copy that has only what came after it.
//!
//! A call that failed (`is_error` on its result) did nothing.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use serde_json::Value;

use crate::schedules::rule::{self, Zone};
use crate::util::parse_rfc3339_ms;

/// How long a recurring job lives after its call (Claude Code's `recurringMaxAgeMs`).
const RECURRING_MAX_AGE_MS: i64 = 7 * 86_400_000;

/// The next time after `now` the session wakes itself, if anything it scheduled is still to
/// come, by its transcripts (one in each folder it was resumed in, all read, the oldest first)
/// and when its process started (`since`).
pub fn next_wakeup(transcripts: &[PathBuf], since: i64, now: i64) -> Option<i64> {
    let mut files: Vec<&PathBuf> = transcripts.iter().collect();
    files.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    let mut scan = Scan::default();
    for path in files {
        let Ok(file) = std::fs::File::open(path) else { continue };
        for line in BufReader::new(file).split(b'\n').map_while(Result::ok) {
            if let Ok(line) = std::str::from_utf8(&line) {
                scan.line(line);
            }
        }
    }
    scan.next_after(now, since, &rule::Local)
}

/// A job scheduled with `CronCreate`.
struct Job {
    id: String,
    cron: String,
    made: i64,
    recurring: bool,
    durable: bool,
    /// When a one-shot job fires, where its result says.
    fires: Option<i64>,
}

/// A call whose result is read.
#[derive(Clone)]
enum Call {
    Cron,
    /// A `CronDelete` of this job.
    Delete(String),
    Wakeup,
}

/// What a transcript's lines, read in order, say was scheduled.
#[derive(Default)]
struct Scan {
    /// `CronCreate` calls by tool use ID: the cron and when it was called.
    crons: Vec<(String, String, i64)>,
    calls: HashMap<String, Call>,
    /// `CronCreate`'s results, by tool use ID.
    results: HashMap<String, Value>,
    deleted: HashSet<String>,
    carried: Vec<Job>,
    /// When each job last fired, by its ID.
    fired: HashMap<String, i64>,
    /// When the latest wakeup is due, while it hasn't fired or been cancelled.
    wakeup: Option<i64>,
}

impl Scan {
    fn line(&mut self, line: &str) {
        // Almost no line is one of these, so only one that may be is parsed.
        let named = ["\"CronCreate\"", "\"CronDelete\"", "\"ScheduleWakeup\"", "scheduled_task_fire", "session_cron_carry"].iter().any(|w| line.contains(w));
        if !named && !(line.contains("toolUseResult") && self.calls.keys().any(|id| line.contains(id.as_str()))) {
            return;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { return };
        let at = v["timestamp"].as_str().and_then(parse_rfc3339_ms);
        let parts = v["message"]["content"].as_array().map(Vec::as_slice).unwrap_or_default();
        match v["type"].as_str() {
            Some("assistant") => {
                for p in parts.iter().filter(|p| p["type"] == "tool_use") {
                    let (Some(id), input) = (p["id"].as_str(), &p["input"]) else { continue };
                    let call = match p["name"].as_str() {
                        Some("CronCreate") => {
                            let (Some(cron), Some(at)) = (input["cron"].as_str(), at) else { continue };
                            self.crons.push((id.to_string(), cron.to_string(), at));
                            Call::Cron
                        }
                        Some("CronDelete") => {
                            let Some(job) = input["id"].as_str() else { continue };
                            Call::Delete(job.to_string())
                        }
                        Some("ScheduleWakeup") => Call::Wakeup,
                        _ => continue,
                    };
                    self.calls.insert(id.to_string(), call);
                }
            }
            Some("user") if v["toolUseResult"].is_object() => {
                for p in parts.iter().filter(|p| p["type"] == "tool_result" && p["is_error"] != true) {
                    let Some(id) = p["tool_use_id"].as_str() else { continue };
                    let result = &v["toolUseResult"];
                    match self.calls.get(id).cloned() {
                        Some(Call::Cron) => {
                            self.results.insert(id.to_string(), result.clone());
                        }
                        Some(Call::Delete(job)) => {
                            self.deleted.insert(job);
                        }
                        Some(Call::Wakeup) => self.wakeup = if result["stopped"] == true { None } else { result["scheduledFor"].as_i64().filter(|t| *t > 0) },
                        None => {}
                    }
                }
            }
            Some("system") if v["subtype"] == "scheduled_task_fire" => {
                if v["taskKind"] == "loop" {
                    self.wakeup = None;
                }
                if let (Some(job), Some(at)) = (v["taskId"].as_str(), at) {
                    self.fired.insert(job.to_string(), at);
                }
            }
            Some("attachment") if v["attachment"]["type"] == "session_cron_carry" => {
                let carry = &v["attachment"];
                if let Some(tasks) = carry["tasks"].as_array() {
                    self.carried = tasks
                        .iter()
                        .filter_map(|t| Some(Job { id: t["id"].as_str()?.to_string(), cron: t["cron"].as_str()?.to_string(), made: t["createdAt"].as_f64()? as i64, recurring: t["recurring"] == true, durable: false, fires: None }))
                        .collect();
                }
                self.deleted.extend(carry["ended"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string));
                if let Some(due) = carry["wakeup"]["scheduledFor"].as_i64() {
                    self.wakeup = Some(due);
                }
            }
            _ => {}
        }
    }

    /// The earliest of what's still to come after `now` for a process started at `since`, on
    /// `zone`'s clock.
    fn next_after(&self, now: i64, since: i64, zone: &dyn Zone) -> Option<i64> {
        let made = self.crons.iter().filter_map(|(call, cron, made)| {
            let r = self.results.get(call)?;
            Some(Job { id: r["id"].as_str()?.to_string(), cron: cron.clone(), made: *made, recurring: r["recurring"] != false, durable: r["durable"] == true, fires: r["nextFireAt"].as_f64().map(|t| t as i64) })
        });
        let mut seen = HashSet::new();
        let jobs = made.chain(self.carried.iter().map(|j| Job { id: j.id.clone(), cron: j.cron.clone(), ..*j })).filter(|j| !self.deleted.contains(&j.id) && seen.insert(j.id.clone()));
        let fires = jobs.filter_map(|j| {
            // A cron Codealong's schedules can't read is one whose time can't be told, and isn't
            // counted: Claude Code takes cron's five fields, which they read.
            let cron = rule::parse(&format!("cron {}", j.cron)).ok()?;
            if !j.recurring {
                return j.fires.or_else(|| rule::next_after(&cron, j.made, j.made, zone)).filter(|t| *t > now);
            }
            let expires = j.made + RECURRING_MAX_AGE_MS;
            let ended = self.fired.get(&j.id).is_some_and(|t| *t >= expires) || (now >= expires && since >= expires && !j.durable);
            (!ended).then(|| rule::next_after(&cron, now, j.made, zone)).flatten()
        });
        fires.chain(self.wakeup.filter(|t| *t > now)).min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedules::rule::tests::{ms, Fake};
    use serde_json::json;

    fn stamp(t: i64) -> String {
        let (d, rest) = (t.div_euclid(86_400_000), t.rem_euclid(86_400_000));
        let (y, mo, day) = crate::util::civil_from_days(d);
        format!("{y:04}-{mo:02}-{day:02}T{:02}:{:02}:{:02}.000Z", rest / 3_600_000, rest / 60_000 % 60, rest / 1000 % 60)
    }

    fn call(at: i64, id: &str, name: &str, input: Value) -> String {
        json!({"type": "assistant", "timestamp": stamp(at), "message": {"content": [{"type": "tool_use", "id": id, "name": name, "input": input}]}}).to_string()
    }

    fn result(at: i64, id: &str, result: Value) -> String {
        json!({"type": "user", "timestamp": stamp(at), "message": {"content": [{"type": "tool_result", "tool_use_id": id, "content": "…"}]}, "toolUseResult": result}).to_string()
    }

    fn failed(at: i64, id: &str) -> String {
        json!({"type": "user", "timestamp": stamp(at), "message": {"content": [{"type": "tool_result", "tool_use_id": id, "is_error": true, "content": "No such job"}]}, "toolUseResult": {"error": "No such job"}}).to_string()
    }

    fn fired(at: i64, job: &str) -> String {
        json!({"type": "system", "subtype": "scheduled_task_fire", "timestamp": stamp(at), "taskId": job, "content": "Running scheduled task"}).to_string()
    }

    fn next(lines: &[String], since: i64, now: i64) -> Option<i64> {
        let mut scan = Scan::default();
        for l in lines {
            scan.line(l);
        }
        scan.next_after(now, since, &Fake::utc())
    }

    #[test]
    fn a_check_scheduled_for_later_is_still_to_come_until_its_time() {
        let z = Fake::utc();
        let asked = ms(&z, 2026, 10, 23, 16, 50);
        // "Wake back up and check in 48 hours", as the northwind session was asked.
        let lines = [call(asked, "toolu_1", "CronCreate", json!({"cron": "50 16 25 10 *", "prompt": "Check the northwind deploy", "recurring": false})), result(asked, "toolu_1", json!({"id": "a1b2c3d4", "humanSchedule": "50 16 25 10 *", "recurring": false, "durable": false}))];
        let due = ms(&z, 2026, 10, 25, 16, 50);
        assert_eq!(next(&lines, asked, asked + 24 * 3_600_000), Some(due));
        assert_eq!(next(&lines, asked, due + 60_000), None);
        // Where the result says when it fires, that's the time.
        let early = due - 45_000;
        let said = [lines[0].clone(), result(asked, "toolu_1", json!({"id": "a1b2c3d4", "recurring": false, "durable": false, "nextFireAt": early as f64 + 0.25}))];
        assert_eq!(next(&said, asked, asked), Some(early));
        // Deleted, it's gone, but not by a delete that failed or hasn't come back.
        let delete = call(asked + 1000, "toolu_2", "CronDelete", json!({"id": "a1b2c3d4"}));
        assert_eq!(next(&[lines[0].clone(), lines[1].clone(), delete.clone(), result(asked + 1000, "toolu_2", json!({"id": "a1b2c3d4"}))], asked, asked + 2000), None);
        assert_eq!(next(&[lines[0].clone(), lines[1].clone(), delete.clone(), failed(asked + 1000, "toolu_2")], asked, asked + 2000), Some(due));
        assert_eq!(next(&[lines[0].clone(), lines[1].clone(), delete], asked, asked + 2000), Some(due));
        // A create that failed scheduled nothing.
        assert_eq!(next(&[lines[0].clone(), failed(asked, "toolu_1")], asked, asked), None);
    }

    #[test]
    fn a_recurring_job_lives_a_week_and_fires_once_more_after() {
        let z = Fake::utc();
        let asked = ms(&z, 2026, 10, 1, 9, 30);
        let lines = vec![call(asked, "toolu_1", "CronCreate", json!({"cron": "0 9 1 * *", "prompt": "Monthly atlas report", "recurring": true})), result(asked, "toolu_1", json!({"id": "beef0001", "recurring": true, "durable": false}))];
        let (week_on, next_month) = (ms(&z, 2026, 10, 9, 12, 0), ms(&z, 2026, 11, 1, 9, 0));
        assert_eq!(next(&lines, asked, ms(&z, 2026, 10, 3, 12, 0)), Some(next_month));
        // Past its week, the process that held it still does until it fires once more; one
        // resumed since doesn't have it.
        assert_eq!(next(&lines, asked, week_on), Some(next_month));
        assert_eq!(next(&lines, week_on - 3_600_000, week_on), None);
        assert_eq!(next(&[lines.clone(), vec![fired(next_month, "beef0001")]].concat(), asked, next_month + 60_000), None);
        // A durable one is reloaded from its project's file by a process started after its week.
        let durable = [lines[0].clone(), result(asked, "toolu_1", json!({"id": "beef0001", "recurring": true, "durable": true}))];
        assert_eq!(next(&durable, week_on - 3_600_000, week_on), Some(next_month));
        assert_eq!(next(&[durable.to_vec(), vec![fired(next_month, "beef0001")]].concat(), week_on - 3_600_000, next_month + 60_000), None);
        // A fire within its week isn't the last.
        let weekly = [call(asked, "toolu_3", "CronCreate", json!({"cron": "0 9 * * 5", "prompt": "Weekly kestrel digest", "recurring": true})), result(asked, "toolu_3", json!({"id": "beef0002", "recurring": true})), fired(ms(&z, 2026, 10, 2, 9, 0), "beef0002")];
        assert_eq!(next(&weekly, asked, ms(&z, 2026, 10, 3, 12, 0)), Some(ms(&z, 2026, 10, 9, 9, 0)));
        // A compaction's carry has it too, and says when it ended.
        let carry = |ended: Value| json!({"type": "attachment", "timestamp": stamp(asked + 1000), "attachment": {"type": "session_cron_carry", "tasks": [{"id": "beef0001", "cron": "0 9 1 * *", "prompt": "Monthly atlas report", "createdAt": asked, "recurring": true}], "ended": ended}}).to_string();
        assert_eq!(next(&[carry(json!([]))], asked, ms(&z, 2026, 10, 3, 12, 0)), Some(next_month));
        assert_eq!(next(&[lines[0].clone(), lines[1].clone(), carry(json!(["beef0001"]))], asked, asked + 2000), None);
    }

    #[test]
    fn the_latest_wakeup_counts_until_it_fires_or_is_stopped() {
        let t = 1_800_000_000_000;
        let wake = |at: i64, id: &str, due: i64| [call(at, id, "ScheduleWakeup", json!({"delaySeconds": 3600, "prompt": "Check the kestrel build", "reason": "fallback"})), result(at, id, json!({"scheduledFor": due, "clampedDelaySeconds": 3600, "wasClamped": false}))];
        let mut lines: Vec<String> = wake(t, "toolu_1", t + 3_600_000).into();
        assert_eq!(next(&lines, t, t + 60_000), Some(t + 3_600_000));
        // A later one takes its place.
        lines.extend(wake(t + 60_000, "toolu_2", t + 1_800_000));
        assert_eq!(next(&lines, t, t + 120_000), Some(t + 1_800_000));
        let tick = json!({"type": "system", "subtype": "scheduled_task_fire", "timestamp": stamp(t + 1_800_000), "taskKind": "loop", "taskId": "1f93c707"}).to_string();
        assert_eq!(next(&[lines.clone(), vec![tick]].concat(), t, t + 1_000_000), None);
        let stopped = [call(t + 90_000, "toolu_3", "ScheduleWakeup", json!({"stop": true})), result(t + 90_000, "toolu_3", json!({"scheduledFor": 0, "stopped": true, "cancelledWakeups": 1}))];
        assert_eq!(next(&[lines, stopped.into()].concat(), t, t + 120_000), None);
    }

    #[test]
    fn every_transcript_of_a_session_is_read() {
        let t = crate::util::now_ms();
        let dir = crate::store::test_dir("wakeups");
        let (first, second) = (dir.join("first.jsonl"), dir.join("second.jsonl"));
        // Scheduled in the folder it ran in first, then resumed in another.
        let lines = [call(t - 60_000, "toolu_1", "CronCreate", json!({"cron": "0 9 * * *", "prompt": "Check northwind", "recurring": false})), result(t - 60_000, "toolu_1", json!({"id": "a1b2c3d4", "recurring": false, "nextFireAt": t + 3_600_000}))];
        std::fs::write(&first, lines.join("\n") + "\n").unwrap();
        std::fs::write(&second, json!({"type": "user", "timestamp": stamp(t), "message": {"content": "hello"}}).to_string() + "\n").unwrap();
        assert_eq!(next_wakeup(&[second.clone(), first.clone()], t - 60_000, t), Some(t + 3_600_000));
        assert_eq!(next_wakeup(&[second], t - 60_000, t), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
