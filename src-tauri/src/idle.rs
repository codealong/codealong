//! Closing the Ghostty terminals of sessions that sat idle a long while and want nothing from
//! you. It's a one-off action: `plan` says what would close and why, and `close` closes the ones
//! you kept ticked, checking each again first. Closing ends the agent's process; its conversation
//! stays on disk and resumes like any other.
//!
//! A terminal closes only when every session in it is idle or unread, has been quiet for the
//! whole wait, left nothing running in the background, and its last reply asks you nothing: by
//! Jev's read when `[jev] sessions` has one, else by the reply not ending in a question.
//!
//! A session's menu closes that one session (`close_one`) without those checks: you picked it.

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
        Status::Error => return Err("last turn failed".into()),
        Status::Working => return Err("working".into()),
        Status::Idle | Status::Unread | Status::Stopped => {}
    }
    if s.background.is_some() {
        return Err("running a command in the background".into());
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
        Some((p, _)) if p >= ASKS_YOU_KEEP => return Err("last reply may ask you something".into()),
        Some((_, Some("blocked"))) => return Err("says it's blocked".into()),
        Some((_, landing)) => why.push(match landing.filter(|l| *l != "none") {
            Some(l) => format!("Jev: asks nothing, work {l}"),
            None => "Jev: asks nothing".into(),
        }),
        None if s.reply_tail.as_deref().is_some_and(|t| t.trim_end().ends_with('?')) => return Err("last reply asks a question".into()),
        None => why.push("last reply asks nothing".into()),
    }
    Ok(why.join(" · "))
}

/// What would close: every live terminal session on the board, grouped by process (one
/// terminal can show several OpenCode sessions), closing only where all of them agree.
/// `closable` says whether a process runs in a tab Codealong can close: Ghostty's, iTerm2's or
/// Terminal's.
pub fn plan(board: &Board, now: i64, after_ms: i64, closable: impl Fn(u32) -> bool) -> Plan {
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
        if !closable(pid) {
            out.kept.extend(sessions.iter().map(|(s, _)| kept(s, format!("not in a {} tab", crate::launch::closes_tabs_in()))));
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

/// The plan for the board as it is now, with where each process runs asked of the kernel.
pub fn plan_now(board: &Board, after_ms: i64) -> Plan {
    plan(board, crate::util::now_ms(), after_ms, |pid| crate::launch::terminal_kind(pid).is_some_and(|t| t.closes_tabs()))
}

/// Closes the terminals of `keys` that the board, looked at again now, still says can close.
pub fn close(board: &Board, keys: &[String], after_ms: i64) -> Vec<Outcome> {
    let fresh = plan_now(board, after_ms);
    let chosen: Vec<&Idle> = keys.iter().filter_map(|k| fresh.close.iter().find(|c| &c.key == k)).collect();
    let targets: Vec<(u32, &std::path::Path)> = chosen.iter().map(|c| (c.pid, std::path::Path::new(c.cwd.as_str()))).collect();
    let results = crate::launch::close_session_terminals(&targets);
    keys.iter()
        .map(|key| {
            if let Some(n) = chosen.iter().position(|c| &c.key == key) {
                let title = chosen[n].title.clone();
                return match &results[n] {
                    Ok(()) => Outcome { key: key.clone(), title, closed: true, message: "Closed.".into() },
                    Err(e) => Outcome { key: key.clone(), title, closed: false, message: e.to_string() },
                };
            }
            let title = board.session(key).map(|(_, s)| s.title.clone()).unwrap_or_default();
            let why = fresh.kept.iter().find(|k| &k.key == key).map(|k| format!("Kept open: {}.", k.why)).unwrap_or_else(|| "Its terminal is already closed.".into());
            Outcome { key: key.clone(), title, closed: false, message: why }
        })
        .collect()
}

/// Closes one session because you picked Close from its menu, whatever it's doing: its terminal
/// tab closes with it, or, for a background job, its harness stops it. Its conversation stays
/// and resumes like any other. Returns what happened, in a sentence.
pub fn close_one(board: &Board, cfg: &crate::config::Config, key: &str) -> anyhow::Result<String> {
    use anyhow::{anyhow, bail};
    let (s, account) = crate::harness::find(board, cfg, key)?;
    if !s.live {
        bail!("It isn't running.");
    }
    if s.attach_command.is_some() {
        return crate::harness::adapter(s.harness).stop(&crate::harness::Ctx { account }, s);
    }
    let pid = match (s.surface, s.pid) {
        (Surface::Cli, Some(pid)) if !s.hosted => pid,
        _ if s.hosted => bail!("Codealong runs it itself, in no terminal."),
        (Surface::App, _) => bail!("It runs in {}; close it there.", crate::harness::adapter(s.harness).app()),
        _ => bail!("It has no terminal Codealong can close."),
    };
    let result = crate::launch::close_session_terminals(&[(pid, std::path::Path::new(&s.cwd))]).pop().unwrap_or_else(|| Err(anyhow!("Nothing was closed.")));
    result.map(|()| format!("Closed the terminal of “{}”. Its conversation is kept and resumes like any other.", s.title))
}

/// What being done with a session ends of it (`actions::done`).
#[derive(Debug, Clone, PartialEq)]
pub enum Ending {
    /// Its terminal tab closes, and its agent's process with it.
    Terminal(u32),
    /// Its background job stops.
    Job,
    /// Its turn stops: a session Codealong runs, whose process runs other sessions too.
    Turn,
    /// Nothing; why, when that's worth saying.
    Nothing(Option<String>),
}

/// What being done with `s` ends, as Close does from its menu, less what would end more than it
/// or end work you may not have meant to. Work under way (a turn, or a command its turn left
/// running in the background) ends only with `stop`: until then it's `Err` with what's under
/// way, and nothing is done. A terminal that shows other running sessions too stays open (one
/// OpenCode terminal can show several), and an app's session stays open in the app, which has
/// no way for another program to close it.
pub fn ending(board: &Board, s: &Session, stop: bool) -> Result<Ending, String> {
    if !s.live {
        return Ok(Ending::Nothing(None));
    }
    if !stop {
        if s.status == Status::Working {
            return Err(format!("“{}” is still working.", s.title));
        }
        if let Some(command) = &s.background {
            let what = if command.trim().is_empty() { "a command".to_string() } else { format!("`{}`", crate::util::truncate(command.trim(), 60)) };
            return Err(format!("“{}” left {what} running in the background.", s.title));
        }
    }
    if s.hosted {
        return Ok(if s.status == Status::Working { Ending::Turn } else { Ending::Nothing(None) });
    }
    if s.attach_command.is_some() {
        return Ok(Ending::Job);
    }
    match (s.surface, s.pid) {
        (Surface::App, _) => Ok(Ending::Nothing(Some(format!("It's still open in {}.", crate::harness::adapter(s.harness).app())))),
        (Surface::Cli, Some(pid)) => {
            let other = board.lanes.iter().flat_map(|l| &l.sessions).find(|o| o.key != s.key && o.live && o.pid == Some(pid));
            Ok(match other {
                Some(o) => Ending::Nothing(Some(format!("Its terminal stays open: it shows “{}” too.", o.title))),
                None => Ending::Terminal(pid),
            })
        }
        _ => Ok(Ending::Nothing(Some("It has no terminal Codealong can close.".into()))),
    }
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
    use crate::model::SessionInsight;

    const NOW: i64 = 100 * DAY_MS;

    fn session(key: &str, pid: u32, status: Status, quiet_ms: i64) -> Session {
        Session { status, pid: Some(pid), title: format!("Session {key}"), cwd: "/work".into(), reply_tail: Some("Pushed it.".into()), last_activity: NOW - quiet_ms, ..crate::fixtures::session(key) }
    }

    fn board(sessions: Vec<Session>) -> Board {

        let mut b = Board::default();
        b.lanes = vec![crate::fixtures::lane("/work", sessions)];
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
        assert_eq!(verdict(&asks, NOW, day), Err("last reply asks a question".into()));
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
        assert_eq!(kept, [("shared-old", "working"), ("shared-busy", "working"), ("tmux", "not in a Ghostty, iTerm2 or Terminal tab")]);
    }

    #[test]
    fn done_ends_only_what_is_its_own_and_not_work_under_way() {
        let lone = session("lone", 1, Status::Unread, 0);
        let shared = session("shared", 2, Status::Idle, 0);
        let mut busy = session("busy", 3, Status::Working, 0);
        busy.title = "Fix login".into();
        let mut left = session("left", 4, Status::Idle, 0);
        left.background = Some("npm run dev".into());
        let mut job = session("job", 5, Status::Idle, 0);
        job.attach_command = Some("claude attach 5".into());
        let mut app = session("app", 6, Status::Idle, 0);
        app.surface = Surface::App;
        let mut gone = session("gone", 7, Status::Idle, 0);
        gone.live = false;
        gone.background = Some("npm run dev".into());
        let mut hosted = session("hosted", 8, Status::Working, 0);
        hosted.hosted = true;
        let mut no_pid = session("no-pid", 9, Status::Idle, 0);
        no_pid.pid = None;
        let b = board(vec![lone.clone(), shared.clone(), session("shared-too", 2, Status::Idle, 0), busy.clone(), left.clone(), job.clone(), app.clone(), gone.clone(), hosted.clone(), no_pid.clone()]);

        assert_eq!(ending(&b, &lone, false), Ok(Ending::Terminal(1)));
        assert_eq!(ending(&b, &shared, false), Ok(Ending::Nothing(Some("Its terminal stays open: it shows “Session shared-too” too.".into()))));
        assert_eq!(ending(&b, &job, false), Ok(Ending::Job));
        assert_eq!(ending(&b, &app, false), Ok(Ending::Nothing(Some("It's still open in the Claude app.".into()))));
        assert_eq!(ending(&b, &gone, false), Ok(Ending::Nothing(None)));
        assert_eq!(ending(&b, &no_pid, false), Ok(Ending::Nothing(Some("It has no terminal Codealong can close.".into()))));
        // Work under way ends only when asked to.
        assert_eq!(ending(&b, &busy, false), Err("“Fix login” is still working.".into()));
        assert_eq!(ending(&b, &busy, true), Ok(Ending::Terminal(3)));
        assert_eq!(ending(&b, &left, false), Err("“Session left” left `npm run dev` running in the background.".into()));
        assert_eq!(ending(&b, &left, true), Ok(Ending::Terminal(4)));
        // A session Codealong runs shares its process: only its turn stops.
        assert!(ending(&b, &hosted, false).is_err());
        assert_eq!(ending(&b, &hosted, true), Ok(Ending::Turn));
        hosted.status = Status::Idle;
        assert_eq!(ending(&b, &hosted, false), Ok(Ending::Nothing(None)));
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
