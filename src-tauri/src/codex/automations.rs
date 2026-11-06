//! The Codex app's own automations, for the Schedules view to list beside Codealong's, read only.
//! Read from the app's code (ChatGPT.app 26.1013, the Codex app's current name, and Codex.app
//! 26.831) and this Mac's files, 2026-11-06:
//!
//! - Each is `<CODEX_HOME>/automations/<id>/automation.toml` (`version = 1`, `id` the folder's
//!   name, `name`, `prompt`, `status`, `rrule`, `created_at` and `updated_at` in ms, optional
//!   `model`, `reasoning_effort`, `notification_policy`). `kind` is `heartbeat` or `cron`, `cron`
//!   when absent. A heartbeat sends its prompt into one thread, `target_thread_id` (one active
//!   heartbeat a thread; the turn comes as `<heartbeat>…`, rollout.rs `request_of`). A cron starts a
//!   new thread each run, in `cwds` (one each) or a `target` project, with an `execution_environment`
//!   (`worktree` when absent, or `local`), and a `memory.md` beside its toml. A cron with no
//!   `cwds` and a heartbeat with no thread are dropped by the app, and so here.
//! - `status` is `ACTIVE`, `PAUSED` or `DELETED` (left out, as the app leaves it out).
//! - `rrule` is an iCalendar rule without `DTSTART` (rrule.js), local wall-clock times: the app
//!   writes `FREQ=MINUTELY;INTERVAL=n`, `FREQ=HOURLY;INTERVAL=n;BYMINUTE=0`,
//!   `FREQ=DAILY;BYHOUR=h;BYMINUTE=m`, `FREQ=WEEKLY;BYDAY=MO,…;BYHOUR;BYMINUTE`,
//!   `FREQ=MONTHLY;BYMONTHDAY=d;…`, and `COUNT=1` for once; `FREQ=HOURLY;INTERVAL=24;BYMINUTE=0`
//!   when absent. Runs are shifted by a few minutes' jitter.
//! - When each runs next isn't in the toml: the app keeps it in `<CODEX_HOME>/sqlite/codex.db` (or
//!   `codex-dev.db`, which is the one on this Mac), table `automations`, `next_run_at` (null while
//!   paused), refreshed as the app lists them.
//! - They run in the app's own process, a tick every 30 seconds, so only while the app is open; a
//!   time that came while it was closed runs at its next tick (from its code, not tried). Neither
//!   the `codex` command nor its app-server keeps or runs them.
//! - `codex://threads/<id>` opens a heartbeat's thread, and in ChatGPT.app
//!   `codex://automations/<id>?automationSource=local` opens one automation (Codex.app 26.831
//!   has only `codex://automations`).
//! - Not read: an account enrolled to keep its automations at chatgpt.com moves them out of the
//!   toml files into the database with its `account_id`; none here had.

use std::path::Path;

use crate::config::AccountCfg;
use crate::model::{Harness, Session};
use crate::schedules::harnesses::Kept;

/// The account's automations, the active ones first: each with the session a heartbeat sends to,
/// where that's one of `sessions`.
pub fn kept(account: &AccountCfg, sessions: &[&Session]) -> Vec<Kept> {
    let home = account.home_path();
    let Ok(dirs) = std::fs::read_dir(home.join("automations")) else { return Vec::new() };
    let next = next_runs(&home);
    let mut out: Vec<Kept> = dirs
        .flatten()
        .filter(|d| d.path().is_dir())
        .filter_map(|d| std::fs::read_to_string(d.path().join("automation.toml")).ok())
        .filter_map(|text| of(&text, account, sessions, &next))
        .collect();
    out.sort_by(|a, b| (a.paused, &a.name).cmp(&(b.paused, &b.name)));
    out
}

/// One automation as its toml says it; none for one the app wouldn't run (deleted, or a heartbeat
/// with no thread).
fn of(text: &str, account: &AccountCfg, sessions: &[&Session], next: &std::collections::HashMap<String, i64>) -> Option<Kept> {
    let t: toml::Table = text.parse().ok()?;
    let s = |k: &str| t.get(k).and_then(toml::Value::as_str).unwrap_or_default().trim().to_string();
    let id = s("id");
    let status = s("status");
    if id.is_empty() || status == "DELETED" {
        return None;
    }
    let heartbeat = s("kind") == "heartbeat";
    let thread = s("target_thread_id");
    if heartbeat && thread.is_empty() {
        return None;
    }
    let paused = status == "PAUSED";
    let cwds: Vec<String> = t.get("cwds").and_then(toml::Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let session = heartbeat.then(|| Session::key_for(Harness::Codex, &account.name, &thread)).filter(|k| sessions.iter().any(|x| &x.key == k));
    let rrule = Some(s("rrule")).filter(|r| !r.is_empty()).unwrap_or_else(|| "FREQ=HOURLY;INTERVAL=24;BYMINUTE=0".into());
    let name = Some(s("name")).filter(|n| !n.is_empty()).unwrap_or_else(|| id.clone());
    Some(Kept {
        key: format!("{}:{}:{id}", Harness::Codex.config(), account.name),
        harness: Harness::Codex,
        account: account.name.clone(),
        kind: if heartbeat { "Heartbeat".into() } else { "Automation".into() },
        name,
        prompt: s("prompt"),
        when_words: rrule_words(&rrule),
        next_at: next.get(&id).copied().filter(|_| !paused),
        paused,
        session,
        folders: cwds,
        link: Some(if heartbeat { crate::launch::codex_thread_url(&thread) } else { format!("codex://automations/{}?automationSource=local", urlencoding::encode(&id)) }),
        lasts: if heartbeat {
            "The Codex app sends it to its thread while the app is open.".into()
        } else {
            "The Codex app starts a new thread for it while the app is open.".into()
        },
        id,
    })
}

/// When each automation runs next, by its id, as the app last noted it in its database.
fn next_runs(home: &Path) -> std::collections::HashMap<String, i64> {
    let mut out = std::collections::HashMap::new();
    for name in ["codex.db", "codex-dev.db"] {
        let Some(db) = crate::harness::open_database(&home.join("sqlite").join(name)) else { continue };
        let Ok(mut q) = db.prepare("SELECT id, next_run_at FROM automations WHERE next_run_at IS NOT NULL") else { continue };
        let rows = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)));
        for (id, at) in rows.into_iter().flatten().flatten() {
            out.entry(id).or_insert(at);
        }
    }
    out
}

/// An rrule in words, for the rules the app writes; any other as it's written.
pub fn rrule_words(rule: &str) -> String {
    let parts: std::collections::HashMap<String, String> = rule.trim().trim_start_matches("RRULE:").split(';').filter_map(|p| p.split_once('=')).map(|(k, v)| (k.trim().to_uppercase(), v.trim().to_string())).collect();
    let n = parts.get("INTERVAL").and_then(|i| i.parse::<u32>().ok()).unwrap_or(1);
    let every = |one: &str, many: &str| if n == 1 { format!("Every {one}") } else { format!("Every {n} {many}") };
    let at = || match (parts.get("BYHOUR").and_then(|h| h.parse::<i64>().ok()), parts.get("BYMINUTE").and_then(|m| m.parse::<i64>().ok())) {
        (Some(h), m) => format!(" at {}", crate::schedules::rule::clock(h * 60 + m.unwrap_or(0))),
        _ => String::new(),
    };
    if parts.get("COUNT").map(String::as_str) == Some("1") {
        return format!("Once{}", at());
    }
    match parts.get("FREQ").map(String::as_str) {
        Some("MINUTELY") => every("minute", "minutes"),
        Some("HOURLY") if n % 24 == 0 => {
            let days = n / 24;
            if days == 1 { "Every day".into() } else { format!("Every {days} days") }
        }
        Some("HOURLY") => every("hour", "hours"),
        Some("DAILY") => format!("{}{}", every("day", "days"), at()),
        Some("WEEKLY") => match parts.get("BYDAY") {
            Some(days) if days == "MO,TU,WE,TH,FR" => format!("Weekdays{}", at()),
            Some(days) => format!("{}{}", days.split(',').map(day_word).collect::<Vec<_>>().join(", "), at()),
            None => format!("{}{}", every("week", "weeks"), at()),
        },
        Some("MONTHLY") => match parts.get("BYMONTHDAY") {
            Some(d) => format!("Monthly on the {d}{}{}", ordinal(d), at()),
            None => format!("{}{}", every("month", "months"), at()),
        },
        _ => rule.trim().to_string(),
    }
}

fn day_word(d: &str) -> &str {
    match d.trim() {
        "MO" => "Mon",
        "TU" => "Tue",
        "WE" => "Wed",
        "TH" => "Thu",
        "FR" => "Fri",
        "SA" => "Sat",
        "SU" => "Sun",
        other => other,
    }
}

fn ordinal(d: &str) -> &'static str {
    match d.parse::<u32>().unwrap_or(0) {
        n if (11..=13).contains(&(n % 100)) => "th",
        n if n % 10 == 1 => "st",
        n if n % 10 == 2 => "nd",
        n if n % 10 == 3 => "rd",
        _ => "th",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account() -> AccountCfg {
        AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false }
    }

    #[test]
    fn says_the_rules_the_app_writes() {
        assert_eq!(rrule_words("FREQ=MINUTELY;INTERVAL=5"), "Every 5 minutes");
        assert_eq!(rrule_words("FREQ=HOURLY;INTERVAL=1;BYMINUTE=50"), "Every hour");
        assert_eq!(rrule_words("FREQ=HOURLY;INTERVAL=24;BYMINUTE=0"), "Every day");
        assert_eq!(rrule_words("FREQ=DAILY;BYHOUR=9;BYMINUTE=30"), "Every day at 9:30");
        assert_eq!(rrule_words("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR;BYHOUR=8;BYMINUTE=0"), "Weekdays at 8:00");
        assert_eq!(rrule_words("FREQ=WEEKLY;BYDAY=MO,TH;BYHOUR=17;BYMINUTE=5"), "Mon, Thu at 17:05");
        assert_eq!(rrule_words("FREQ=MONTHLY;BYMONTHDAY=2;BYHOUR=9;BYMINUTE=0"), "Monthly on the 2nd at 9:00");
        assert_eq!(rrule_words("FREQ=DAILY;COUNT=1;BYHOUR=15;BYMINUTE=0"), "Once at 15:00");
        assert_eq!(rrule_words("FREQ=YEARLY;BYMONTH=3"), "FREQ=YEARLY;BYMONTH=3");
    }

    #[test]
    fn reads_an_automation_as_the_app_writes_it() {
        let heartbeat = r#"
version = 1
id = "watch-atlas-deploy"
kind = "heartbeat"
name = "Watch the atlas deploy"
prompt = "Check the atlas deploy; pause this heartbeat once it's out."
status = "ACTIVE"
rrule = "FREQ=MINUTELY;INTERVAL=5"
target_thread_id = "01a0-thread"
created_at = 1789399292341
updated_at = 1789399292341
"#;
        let thread = Session::new(Harness::Codex, "codex", "01a0-thread");
        let next = std::collections::HashMap::from([("watch-atlas-deploy".to_string(), 1_789_400_000_000)]);
        let k = of(heartbeat, &account(), &[&thread], &next).unwrap();
        assert_eq!((k.key.as_str(), k.kind.as_str(), k.when_words.as_str(), k.next_at, k.paused), ("codex:codex:watch-atlas-deploy", "Heartbeat", "Every 5 minutes", Some(1_789_400_000_000), false));
        assert_eq!(k.session.as_deref(), Some("codex:codex:01a0-thread"));
        assert_eq!(k.link.as_deref(), Some("codex://threads/01a0-thread"));
        // Its thread not on the board: still listed, with its link.
        assert_eq!(of(heartbeat, &account(), &[], &next).unwrap().session, None);
        // Paused: no next time. Deleted, or a heartbeat with no thread: not listed.
        let paused = of(&heartbeat.replace("ACTIVE", "PAUSED"), &account(), &[], &next).unwrap();
        assert_eq!((paused.paused, paused.next_at), (true, None));
        assert!(of(&heartbeat.replace("ACTIVE", "DELETED"), &account(), &[], &next).is_none());
        assert!(of(&heartbeat.replace("01a0-thread", ""), &account(), &[], &next).is_none());
        // A cron (the kind when none is said) starts a thread in its folder each run.
        let cron = "version = 1\nid = \"nightly\"\nname = \"Nightly triage\"\nprompt = \"Triage\"\nstatus = \"ACTIVE\"\nrrule = \"FREQ=DAILY;BYHOUR=2;BYMINUTE=0\"\ncwds = [\"/Users/me/projects/northwind\"]\n";
        let k = of(cron, &account(), &[], &next).unwrap();
        assert_eq!((k.kind.as_str(), k.folders.clone(), k.link.as_deref()), ("Automation", vec!["/Users/me/projects/northwind".to_string()], Some("codex://automations/nightly?automationSource=local")));
        // One in several folders is in each of their projects.
        let both = cron.replace("cwds = [\"/Users/me/projects/northwind\"]", "cwds = [\"/Users/me/projects/northwind\", \"/Users/me/projects/atlas\"]");
        assert_eq!(of(&both, &account(), &[], &next).unwrap().folders.len(), 2);
    }
}
