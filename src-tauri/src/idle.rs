//! Closing the Ghostty terminals of sessions that sat idle a long while and want nothing from
//! you. It's a one-off action: `plan` says what would close and why, and `close` closes the ones
//! you kept ticked, checking each again first. Closing ends the agent's process; its conversation
//! stays on disk and resumes like any other.
//!
//! A terminal closes only when every session in it is idle or unread, has been quiet for the
//! whole wait, left nothing running in the background, and its last reply asks you nothing: by
//! Jev's read when `[jev] sessions` has one, else by the reply not ending in a question.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::model::{Board, Harness, Session, Status, Surface};
use crate::util::DAY_MS;

/// Jev's likelihood that a reply asks you something, from which the terminal stays. Lower than
/// the board's Needs you mark: a reply that might ask something is worth keeping open.
const ASKS_YOU_KEEP: f64 = 0.3;

/// A terminal that can close, named by its session.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Idle {
    pub key: String,
    pub title: String,
    pub account: String,
    pub harness: Harness,
    pub pid: u32,
    pub cwd: String,
    /// The worktree it's listed under (a lane id), and its short name.
    pub worktree: String,
    pub callsign: String,
    pub last_activity: i64,
    /// Why it can close, in words.
    pub why: String,
}

/// A live terminal session that stays open, and why.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Kept {
    pub key: String,
    pub title: String,
    pub account: String,
    pub why: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub after_ms: i64,
    pub close: Vec<Idle>,
    pub kept: Vec<Kept>,
}

/// What closing one terminal did.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub key: String,
    pub title: String,
    pub closed: bool,
    pub message: String,
}

/// Whether a live terminal session lets its terminal close: `Ok` with why, `Err` with why not.
pub fn verdict(s: &Session, now: i64, after_ms: i64) -> Result<String, String> {
    match s.status {
        Status::Needs => return Err("needs you".into()),
        Status::Error => return Err("its last turn failed".into()),
        Status::Working => return Err("working".into()),
        Status::Idle | Status::Unread | Status::Stopped => {}
    }
    if s.background.is_some() {
        return Err("left a command running in the background".into());
    }
    if s.stalled_since.is_some() {
        return Err("stuck mid-turn".into());
    }
    let quiet = now - s.last_activity;
    if quiet < after_ms {
        return Err(if quiet < 120_000 { "active just now".into() } else { format!("active {} ago", span(quiet)) });
    }
    let mut why = vec![format!("idle {}", span(quiet))];
    if s.status == Status::Unread {
        why.push("last reply unread".into());
    }
    match s.insight.as_ref().and_then(|i| i.asks_you.map(|p| (p, i.landing.as_deref()))) {
        Some((p, _)) if p >= ASKS_YOU_KEEP => return Err("its last reply may ask you something".into()),
        Some((_, Some("blocked"))) => return Err("it says it's blocked".into()),
        Some((_, landing)) => why.push(match landing.filter(|l| *l != "none") {
            Some(l) => format!("Jev: asks nothing, work {l}"),
            None => "Jev: asks nothing".into(),
        }),
        None if s.reply_tail.as_deref().is_some_and(|t| t.trim_end().ends_with('?')) => return Err("its last reply ends with a question".into()),
        None => why.push("last reply asks nothing".into()),
    }
    Ok(why.join(" · "))
}

/// What would close: every live terminal session on the board, grouped by process (one
/// terminal can show several OpenCode sessions), closing only where all of them agree.
/// `in_ghostty` says whether a process runs in a Ghostty tab.
pub fn plan(board: &Board, now: i64, after_ms: i64, in_ghostty: impl Fn(u32) -> bool) -> Plan {
    let mut seen = HashSet::new();
    let mut by_pid: BTreeMap<u32, Vec<(&Session, &crate::model::Lane)>> = BTreeMap::new();
    for lane in &board.lanes {
        for s in &lane.sessions {
            if s.visitor.is_some() || !seen.insert(s.key.as_str()) {
                continue;
            }
            // Background jobs and the apps' sessions have no terminal of their own.
            let Some(pid) = s.pid.filter(|_| s.live && s.surface == Surface::Cli && s.attach_command.is_none()) else { continue };
            by_pid.entry(pid).or_default().push((s, lane));
        }
    }
    let mut out = Plan { after_ms, ..Plan::default() };
    for (pid, sessions) in by_pid {
        let kept = |s: &Session, why: String| Kept { key: s.key.clone(), title: s.title.clone(), account: s.account.clone(), why };
        if !in_ghostty(pid) {
            out.kept.extend(sessions.iter().map(|(s, _)| kept(s, "not in a Ghostty tab".into())));
            continue;
        }
        let verdicts: Vec<_> = sessions.iter().map(|(s, _)| verdict(s, now, after_ms)).collect();
        if let Some(Err(why)) = verdicts.iter().find(|v| v.is_err()) {
            out.kept.extend(sessions.iter().map(|(s, _)| kept(s, why.clone())));
            continue;
        }
        // The newest session names the terminal: it's the one the tab shows.
        let (i, (s, lane)) = sessions.iter().enumerate().max_by_key(|(_, (s, _))| s.last_activity).expect("a group has a session");
        out.close.push(Idle {
            key: s.key.clone(),
            title: s.title.clone(),
            account: s.account.clone(),
            harness: s.harness,
            pid,
            cwd: s.cwd.clone(),
            worktree: lane.id.clone(),
            callsign: lane.callsign.clone(),
            last_activity: s.last_activity,
            why: verdicts[i].clone().unwrap_or_default(),
        });
    }
    out.close.sort_by_key(|c| c.last_activity);
    out
}

/// The plan for the board as it is now, with Ghostty's processes read from `ps`.
pub fn plan_now(board: &Board, after_ms: i64) -> Plan {
    let table = crate::procs::run("/bin/ps", &["-A", "-o", "pid=,ppid=,comm="], None, std::time::Duration::from_secs(3)).unwrap_or_default();
    plan(board, crate::util::now_ms(), after_ms, |pid| crate::launch::runs_under(&table, pid, "ghostty"))
}

/// Closes the terminals of `keys` that the board, looked at again now, still says can close.
pub fn close(board: &Board, keys: &[String], after_ms: i64) -> Vec<Outcome> {
    let fresh = plan_now(board, after_ms);
    keys.iter()
        .map(|key| {
            let title = || board.lanes.iter().flat_map(|l| &l.sessions).find(|s| &s.key == key).map(|s| s.title.clone()).unwrap_or_default();
            let Some(c) = fresh.close.iter().find(|c| &c.key == key) else {
                let why = fresh.kept.iter().find(|k| &k.key == key).map(|k| format!("Kept open: {}.", k.why)).unwrap_or_else(|| "Its terminal is already closed.".into());
                return Outcome { key: key.clone(), title: title(), closed: false, message: why };
            };
            match crate::launch::close_session_terminal(&board.apps.terminal_id, c.pid, std::path::Path::new(&c.cwd)) {
                Ok(()) => Outcome { key: key.clone(), title: c.title.clone(), closed: true, message: "Closed.".into() },
                Err(e) => Outcome { key: key.clone(), title: c.title.clone(), closed: false, message: e.to_string() },
            }
        })
        .collect()
}

/// A stretch of time in words: 26 hours, 3 days.
fn span(ms: i64) -> String {
    let mins = ms.max(0) / 60_000;
    match mins {
        0..=1 => "just now".into(),
        2..=59 => format!("{mins} minutes"),
        60..=119 => "an hour".into(),
        120..=2879 => format!("{} hours", mins / 60),
        _ => format!("{} days", mins / 1440),
    }
}

/// `24h`, `3d`, `90m` or `1w` as milliseconds.
pub fn parse_after(s: &str) -> Option<i64> {
    let (num, unit) = s.split_at(s.trim_end_matches(|c: char| c.is_ascii_alphabetic()).len());
    let n: i64 = num.trim().parse().ok().filter(|n| *n > 0)?;
    Some(match unit {
        "m" => n * 60_000,
        "h" => n * 3_600_000,
        "d" => n * DAY_MS,
        "w" => n * 7 * DAY_MS,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bay, Lane, Origin, SessionInsight};

    const NOW: i64 = 100 * DAY_MS;

    fn session(key: &str, pid: u32, status: Status, quiet_ms: i64) -> Session {
        Session {
            key: key.into(),
            id: key.into(),
            short_id: key.into(),
            harness: Harness::Claude,
            account: "claude".into(),
            surface: Surface::Cli,
            status,
            live: true,
            pid: Some(pid),
            title: format!("Session {key}"),
            cwd: "/work".into(),
            work: Vec::new(),
            reply_tail: Some("Pushed it.".into()),
            first_prompt: None,
            name: None,
            started_at: None,
            status_since: None,
            last_activity: NOW - quiet_ms,
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
            stalled_since: None,
            background: None,
            insight: None,
        }
    }

    fn board(sessions: Vec<Session>) -> Board {
        let lane = Lane {
            id: "/work".into(),
            project: "p".into(),
            repo: "r".into(),
            repo_slug: None,
            path: "/work".into(),
            callsign: "work".into(),
            origin: Origin::Main,
            branch: None,
            detached: None,
            bay: Bay::Idle,
            glyph: Status::Idle,
            parked: false,
            hidden: false,
            headline: String::new(),
            detail: None,
            sessions,
            pr: None,
            git: None,
            last_activity: 0,
            looks_finished: false,
            targets: Vec::new(),
            default_target: None,
        };
        let mut b = Board::default();
        b.lanes = vec![lane];
        b
    }

    #[test]
    fn closes_only_what_sat_quiet_and_asks_nothing() {
        let day = DAY_MS;
        assert!(verdict(&session("a", 1, Status::Idle, 2 * day), NOW, day).unwrap().starts_with("idle 2 days"));
        assert_eq!(verdict(&session("a", 1, Status::Idle, 3 * 3_600_000), NOW, day), Err("active 3 hours ago".into()));
        for status in [Status::Needs, Status::Working, Status::Error] {
            assert!(verdict(&session("a", 1, status, 2 * day), NOW, day).is_err(), "{status:?}");
        }
        let mut bg = session("a", 1, Status::Idle, 2 * day);
        bg.background = Some("gh pr checks --watch".into());
        assert!(verdict(&bg, NOW, day).is_err());

        let mut asks = session("a", 1, Status::Unread, 2 * day);
        asks.reply_tail = Some("Want me to push it?".into());
        assert_eq!(verdict(&asks, NOW, day), Err("its last reply ends with a question".into()));
        // Jev's read wins over the question mark, both ways.
        asks.insight = Some(SessionInsight { asks_you: Some(0.1), landing: Some("landed".into()), ..Default::default() });
        assert_eq!(verdict(&asks, NOW, day).unwrap(), "idle 2 days · last reply unread · Jev: asks nothing, work landed");
        let mut maybe = session("a", 1, Status::Idle, 2 * day);
        maybe.insight = Some(SessionInsight { asks_you: Some(0.4), ..Default::default() });
        assert!(verdict(&maybe, NOW, day).is_err());
        maybe.insight = Some(SessionInsight { asks_you: Some(0.0), landing: Some("blocked".into()), ..Default::default() });
        assert!(verdict(&maybe, NOW, day).is_err());
    }

    #[test]
    fn a_terminal_closes_only_when_all_its_sessions_can() {
        let day = DAY_MS;
        let mut app = session("app", 4, Status::Idle, 3 * day);
        app.surface = Surface::App;
        let mut job = session("job", 5, Status::Idle, 3 * day);
        job.attach_command = Some("claude attach 5".into());
        let mut gone = session("gone", 6, Status::Idle, 3 * day);
        gone.live = false;
        let b = board(vec![
            session("old", 1, Status::Idle, 3 * day),
            session("shared-old", 2, Status::Idle, 3 * day),
            session("shared-busy", 2, Status::Working, 3 * day),
            session("tmux", 3, Status::Idle, 3 * day),
            app,
            job,
            gone,
        ]);
        let p = plan(&b, NOW, day, |pid| pid != 3);
        assert_eq!(p.close.iter().map(|c| c.key.as_str()).collect::<Vec<_>>(), ["old"]);
        let kept: Vec<(&str, &str)> = p.kept.iter().map(|k| (k.key.as_str(), k.why.as_str())).collect();
        assert_eq!(kept, [("shared-old", "working"), ("shared-busy", "working"), ("tmux", "not in a Ghostty tab")]);
    }

    /// What would close on this Mac now, closing nothing:
    /// `cargo test this_macs_idle_terminals -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn this_macs_idle_terminals() {
        let p = plan_now(&crate::engine::light_snapshot(), DAY_MS);
        for c in &p.close {
            println!("close  {:<10} {:<40} {}", c.account, c.title.chars().take(40).collect::<String>(), c.why);
        }
        for k in &p.kept {
            println!("keep   {:<10} {:<40} {}", k.account, k.title.chars().take(40).collect::<String>(), k.why);
        }
    }

    #[test]
    fn reads_waits() {
        assert_eq!(parse_after("24h"), Some(DAY_MS));
        assert_eq!(parse_after("2d"), Some(2 * DAY_MS));
        assert_eq!(parse_after("0h"), None);
        assert_eq!(parse_after("soon"), None);
    }
}
