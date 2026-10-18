//! Closing the sessions that sat idle a long while and want nothing from you. It's a one-off
//! action: `plan` says what would close and why, and `close` closes the ones you kept ticked,
//! checking each again first. A session in a Ghostty, iTerm2 or Terminal tab closes with its tab,
//! which ends the agent's process; a background job is stopped by its harness. Either way its
//! conversation stays on disk and resumes like any other.
//!
//! A session closes only when it's idle or unread, has been quiet for the whole wait, left
//! nothing running in the background, and its last reply asks you nothing: by Jev's read when
//! `[jev] sessions` has one, else by the reply not ending in a question. A terminal closes only
//! when every session in it can.
//!
//! Asked to archive them too, it does to each what Mark as done does (`actions::done`): marks it
//! seen and archives it, so it leaves every list but Archived. What Codealong can't end (an app's
//! session, one it runs itself, one in a terminal it can't close) is then listed to be archived
//! only, and stays open where it is.
//!
//! A session's menu closes that one session (`close_one`) without those checks: you picked it.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::{Board, Harness, Lane, Session, Status, Surface};
use crate::util::DAY_MS;

/// Jev's likelihood that a reply asks you something, from which the session stays. Lower than
/// the board's Needs you mark: a reply that might ask something is worth keeping open.
const ASKS_YOU_KEEP: f64 = 0.3;

/// How a session the plan lists ends.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Ends {
    /// Its terminal tab closes, and its agent's process with it.
    Terminal,
    /// Its background job stops (`Adapter::stop`).
    Job,
    /// Nothing Codealong can do ends it, so it's only archived: listed only when that's asked.
    Archive,
}

/// A session that can close, or a terminal, named by the session it shows.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Idle {
    pub key: String,
    pub title: String,
    pub account: String,
    pub harness: Harness,
    pub ends: Ends,
    /// The process whose terminal closes.
    pub pid: Option<u32>,
    pub cwd: String,
    /// The worktree it's listed under (a lane id), and its short name.
    pub worktree: String,
    pub callsign: String,
    pub last_activity: i64,
    /// Why it can close, in words, and where it stays open when it's only archived.
    pub why: String,
    /// The other sessions its terminal shows (one OpenCode terminal can show several), which
    /// close with it, and are archived with it when that's asked.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub with: Vec<String>,
}

/// A live session that stays open, and why.
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
    /// Whether what closes is archived too, and what can't close is archived only.
    #[serde(default)]
    pub archive: bool,
    pub close: Vec<Idle>,
    pub kept: Vec<Kept>,
}

/// What closing one session did.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub key: String,
    pub title: String,
    /// What was to end it, when the plan still listed it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ends: Option<Ends>,
    /// Its terminal closed or its job stopped.
    pub closed: bool,
    pub archived: bool,
    /// Everything asked of it was done.
    pub ok: bool,
    pub message: String,
}

/// Whether a live session may close: `Ok` with why, `Err` with why not.
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

/// What would close: every live session on the board. A terminal's sessions are grouped by its
/// process (one terminal can show several OpenCode sessions), and it closes only where all of
/// them agree; a background job stops on its own. With `archive`, a session that could close
/// but that Codealong can't end is listed to be archived only. `closable` says whether a process
/// runs in a tab Codealong can close: Ghostty's, iTerm2's or Terminal's.
pub fn plan(board: &Board, now: i64, after_ms: i64, archive: bool, closable: impl Fn(u32) -> bool) -> Plan {
    let mut seen = HashSet::new();
    let mut by_pid: BTreeMap<u32, Vec<(&Session, &Lane)>> = BTreeMap::new();
    let mut out = Plan { after_ms, archive, ..Plan::default() };
    for lane in &board.lanes {
        for s in &lane.sessions {
            if s.visitor.is_some() || !s.live || !seen.insert(s.key.as_str()) {
                continue;
            }
            if s.attach_command.is_some() {
                match verdict(s, now, after_ms) {
                    Ok(why) => out.close.push(idle(s, lane, Ends::Job, why)),
                    Err(why) => out.kept.push(kept(s, why)),
                }
                continue;
            }
            match (s.surface, s.pid) {
                (Surface::Cli, Some(pid)) if !s.hosted => by_pid.entry(pid).or_default().push((s, lane)),
                (Surface::App, _) => out.stays(s, lane, now, format!("in {}", crate::harness::adapter(s.harness).app())),
                _ if s.hosted => out.stays(s, lane, now, "in Codealong".into()),
                _ => out.stays(s, lane, now, NO_TAB.into()),
            }
        }
    }
    for (pid, sessions) in by_pid {
        if !closable(pid) {
            for (s, lane) in sessions {
                out.stays(s, lane, now, NO_TAB.into());
            }
            continue;
        }
        let verdicts: Vec<_> = sessions.iter().map(|(s, _)| verdict(s, now, after_ms)).collect();
        if let Some(i) = verdicts.iter().position(|v| v.is_err()) {
            // One that can't close holds its terminal open for the others too.
            let (holds, held) = (sessions[i].0, verdicts[i].clone().unwrap_err());
            for ((s, lane), v) in sessions.iter().zip(verdicts) {
                match v {
                    Ok(why) if archive => out.close.push(idle(s, lane, Ends::Archive, format!("{why} · its terminal stays open for “{}”", holds.title))),
                    _ => out.kept.push(kept(s, held.clone())),
                }
            }
            continue;
        }
        // The newest session names the terminal: it's the one the tab shows.
        let i = (0..sessions.len()).max_by_key(|&i| sessions[i].0.last_activity).expect("a group has a session");
        let (s, lane) = sessions[i];
        let mut terminal = idle(s, lane, Ends::Terminal, verdicts[i].clone().unwrap_or_default());
        terminal.pid = Some(pid);
        terminal.with = sessions.iter().filter(|(o, _)| o.key != s.key).map(|(o, _)| o.key.clone()).collect();
        out.close.push(terminal);
    }
    out.close.sort_by_key(|c| c.last_activity);
    out
}

/// Where a session in a terminal Codealong can't close stays open: tmux, an editor's terminal.
const NO_TAB: &str = "in a terminal Codealong can't close";

impl Plan {
    /// A session that may close but stays open `place` whatever Codealong does: archived only
    /// when that's asked, else kept, saying where.
    fn stays(&mut self, s: &Session, lane: &Lane, now: i64, place: String) {
        match verdict(s, now, self.after_ms) {
            Ok(why) if self.archive => self.close.push(idle(s, lane, Ends::Archive, format!("{why} · stays open {place}"))),
            Ok(_) => self.kept.push(kept(s, place)),
            Err(why) => self.kept.push(kept(s, why)),
        }
    }
}

fn idle(s: &Session, lane: &Lane, ends: Ends, why: String) -> Idle {
    Idle {
        key: s.key.clone(),
        title: s.title.clone(),
        account: s.account.clone(),
        harness: s.harness,
        ends,
        pid: None,
        cwd: s.cwd.clone(),
        worktree: lane.id.clone(),
        callsign: lane.callsign.clone(),
        last_activity: s.last_activity,
        why,
        with: Vec::new(),
    }
}

fn kept(s: &Session, why: String) -> Kept {
    Kept { key: s.key.clone(), title: s.title.clone(), account: s.account.clone(), why }
}

/// The plan for the board as it is now, with where each process runs asked of the kernel.
pub fn plan_now(board: &Board, after_ms: i64, archive: bool) -> Plan {
    plan(board, crate::util::now_ms(), after_ms, archive, |pid| crate::launch::terminal_kind(pid).is_some_and(|t| t.closes_tabs()))
}

/// Archives sessions, each marked seen first, as Mark as done does: each key's result, in order.
pub type Archiver<'a> = &'a dyn Fn(&[String]) -> Vec<anyhow::Result<()>>;

/// Closes the sessions of `keys` that the board, looked at again now, still says can close, and
/// with `archive` archives them, and the ones listed to be archived only. One outcome a key, and
/// one for each other session a closed terminal showed.
pub fn close(board: &Board, cfg: &crate::config::Config, keys: &[String], after_ms: i64, archive: Option<Archiver>) -> Vec<Outcome> {
    use crate::util::words;
    let fresh = plan_now(board, after_ms, archive.is_some());
    let chosen: Vec<&Idle> = keys.iter().filter_map(|k| fresh.close.iter().find(|c| &c.key == k)).collect();
    // How each ended: the terminals together, each told by a script, then the jobs one by one.
    let mut ended: HashMap<&str, Result<(), String>> = HashMap::new();
    let tabs: Vec<(&Idle, u32)> = chosen.iter().filter(|c| c.ends == Ends::Terminal).filter_map(|c| Some((*c, c.pid?))).collect();
    let targets: Vec<(u32, &Path)> = tabs.iter().map(|(c, pid)| (*pid, Path::new(c.cwd.as_str()))).collect();
    for ((c, _), result) in tabs.iter().zip(crate::launch::close_session_terminals(&targets)) {
        ended.insert(&c.key, result.map_err(words));
    }
    for c in chosen.iter().filter(|c| c.ends == Ends::Job) {
        let stopped = crate::harness::find(board, cfg, &c.key).and_then(|(s, account)| crate::harness::adapter(s.harness).stop(&crate::harness::Ctx { account }, s));
        ended.insert(&c.key, stopped.map(|_| ()).map_err(words));
    }
    // Then what's archived: what ended, with the other sessions its terminal showed, and what
    // only archiving was asked of. What didn't end stays as it was.
    let mut archived: HashMap<String, Result<(), String>> = HashMap::new();
    if let Some(archive) = archive {
        let keys: Vec<String> = chosen
            .iter()
            .filter(|c| c.ends == Ends::Archive || ended.get(c.key.as_str()).is_some_and(|r| r.is_ok()))
            .flat_map(|c| std::iter::once(c.key.clone()).chain(c.with.iter().cloned()))
            .collect();
        if !keys.is_empty() {
            let results = archive(&keys);
            archived = keys.into_iter().zip(results.into_iter().map(|r| r.map_err(words))).collect();
        }
    }
    let title = |key: &str| board.session(key).map(|(_, s)| s.title.clone()).unwrap_or_default();
    let mut out = Vec::new();
    for key in keys {
        let Some(c) = chosen.iter().find(|c| &c.key == key) else {
            let why = fresh.kept.iter().find(|k| &k.key == key).map(|k| format!("Kept open: {}.", k.why)).unwrap_or_else(|| "It isn't running any more.".into());
            out.push(Outcome { key: key.clone(), title: title(key), ends: None, closed: false, archived: false, ok: false, message: why });
            continue;
        };
        for one in std::iter::once(&c.key).chain(&c.with) {
            out.push(outcome(one, title(one), c.ends, ended.get(c.key.as_str()), archive.is_some(), archived.get(one)));
        }
    }
    out
}

/// What closing one session did, with a sentence that says it: `ended` is how its terminal or
/// job went and `archived` how archiving it went, where either was tried; `asked` whether
/// archiving was.
fn outcome(key: &str, title: String, ends: Ends, ended: Option<&Result<(), String>>, asked: bool, archived: Option<&Result<(), String>>) -> Outcome {
    let closed = ended.is_some_and(|r| r.is_ok());
    let is_archived = archived.is_some_and(|r| r.is_ok());
    let did = if ends == Ends::Job { "Stopped" } else { "Closed" };
    let message = match (ends, ended, archived) {
        (Ends::Archive, _, Some(Ok(()))) => "Archived; it stays open where it is.".to_string(),
        (_, Some(Ok(())), Some(Ok(()))) => format!("{did} and archived."),
        (_, Some(Ok(())), Some(Err(e))) => format!("{did}, but it couldn't be archived: {e}"),
        (_, Some(Ok(())), None) => format!("{did}."),
        (_, Some(Err(e)), _) => e.clone(),
        (_, _, Some(Err(e))) => format!("It couldn't be archived: {e}"),
        _ => "Nothing was done.".to_string(),
    };
    let ok = (closed || ends == Ends::Archive) && (is_archived || !asked);
    Outcome { key: key.to_string(), title, ends: Some(ends), closed, archived: is_archived, ok, message }
}

/// `close` for the app, whose engine archives: each session is marked seen, then archived, as
/// Mark as done does.
pub fn close_on(engine: &crate::engine::Handle, keys: &[String], after_ms: i64, archive: bool) -> Result<Vec<Outcome>, String> {
    let cfg = crate::config::current().map_err(crate::util::words)?;
    let archiver = |keys: &[String]| {
        engine.mark_seen(keys.to_vec());
        engine.archive_all(keys)
    };
    Ok(close(&engine.board(), &cfg, keys, after_ms, archive.then_some(&archiver as Archiver)))
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
    /// Its harness's exit command is typed into its terminal (`exit`), so the harness cleans up
    /// the worktree it made for it, and the tab closes once the process has ended.
    Exit(u32),
    /// Its background job stops. Mark as done then removes the worktree its harness made for it
    /// too, when nothing in it would be lost (cleanup.rs `after_job`), which `stop` alone keeps.
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
/// no way for another program to close it. One that holds a worktree its harness made for it
/// exits its own way when it can (`exits_its_own_way`), so that worktree doesn't outlive it.
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
                None if exits_its_own_way(s) => Ending::Exit(pid),
                None => Ending::Terminal(pid),
            })
        }
        _ => Ok(Ending::Nothing(Some("It has no terminal Codealong can close.".into()))),
    }
}

/// Whether being done with `s` types its harness's exit command rather than hanging it up: it
/// holds a worktree its harness made for it (`Session::own_worktree`), which the harness cleans
/// up only as it exits its own way, and nothing under way would take the command for something
/// else. A running turn, stuck or not, would take it as a prompt once it ends, a command left
/// running in the background has Claude Code ask about it first, and a prompt waiting on you
/// would take it as the answer; those hang up as before, and the worktree stays.
fn exits_its_own_way(s: &Session) -> bool {
    s.own_worktree.is_some()
        && crate::harness::adapter(s.harness).exit_command().is_some()
        && s.status != Status::Working
        && s.stalled_since.is_none()
        && s.background.is_none()
        && !crate::send::waits_on_you(s)
}

/// How long a session typed its exit command is given to end before Codealong takes it that
/// its harness is asking you something first. From `/exit` to the end of the process, Claude
/// Code 2.1.292 took 1.7 s to check, remove and shut down in a small repo (2026-10-07).
const EXIT_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// How long it's given once its harness has started removing the worktree, which takes as long
/// as deleting its files does: a worktree with its dependencies installed has many. Under the
/// minute a window shows it archived ahead of the answer (`src/ahead.ts` `LONGEST_MS`), so it
/// doesn't come back meanwhile.
const REMOVAL_WAIT: std::time::Duration = std::time::Duration::from_secs(45);

/// What came of typing a session's exit command (`exit`).
#[derive(Debug, Clone, PartialEq)]
pub enum Exited {
    /// Its process ended. Whether its worktree is gone, and how closing its tab went.
    Ended { removed: bool, closed: Result<(), String> },
    /// It's still running: its harness asks in its terminal whether to keep the worktree, its
    /// tab left open for the answer.
    Asks,
    /// It's still removing the worktree, its tab left open.
    Removing,
}

/// Types the harness's exit command into `s`'s terminal (`Ending::Exit`), clearing its input box
/// first so a draft there doesn't go with it, and waits for the process to end, then closes the
/// tab. `Err` when nothing could be typed: then nothing was done to it.
///
/// Claude Code holds a git lock on its worktree while it runs ("claude session <name> (pid …)")
/// and takes it off just before deleting the worktree; while it asks, the lock stays. So a lock
/// that goes while the process still runs says the worktree is being removed, which is waited
/// for longer than a question is.
pub fn exit(s: &Session, account: &crate::config::AccountCfg, pid: u32) -> Result<Exited, String> {
    let adapter = crate::harness::adapter(s.harness);
    let Some(command) = adapter.exit_command() else { return Err(format!("{} has no command to exit a session.", adapter.harness().word())) };
    let lock = s.own_worktree.as_deref().and_then(|w| crate::repos::linked_gitdir(Path::new(w))).map(|d| d.join("locked")).filter(|l| l.exists());
    let typed = adapter
        .routes(account, s)
        .into_iter()
        .find_map(|route| match route {
            crate::send::Route::Type { pid } => Some(crate::launch::terminal_type_session(pid, Path::new(&s.cwd), command, true, crate::launch::Submit::Return, false)),
            _ => None,
        })
        .unwrap_or_else(|| Ok(crate::launch::Typed::Nowhere("it isn't in a terminal Codealong can type into".into())));
    match typed {
        Ok(crate::launch::Typed::Into(_)) => {}
        Ok(crate::launch::Typed::Nowhere(why)) => return Err(format!("Codealong couldn't type `{command}` into its terminal ({why}).")),
        Err(e) => return Err(format!("Codealong couldn't type `{command}` into its terminal: {}", crate::util::words(e))),
    }
    let started = std::time::Instant::now();
    while crate::procs::pid_alive(pid) {
        let removing = lock.as_ref().is_some_and(|l| !l.exists());
        if started.elapsed() > if removing { REMOVAL_WAIT } else { EXIT_WAIT } {
            return Ok(if removing { Exited::Removing } else { Exited::Asks });
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let removed = s.own_worktree.as_deref().is_some_and(|w| !Path::new(w).exists());
    let closed = crate::launch::close_session_terminals(&[(pid, Path::new(&s.cwd))]).pop().unwrap_or_else(|| Err(anyhow::anyhow!("Nothing was closed.")));
    Ok(Exited::Ended { removed, closed: closed.map_err(crate::util::words) })
}

/// A stretch of time in words: 26 hours, 3 days.
pub fn span(ms: i64) -> String {
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
        let mut busy_job = session("busy-job", 7, Status::Working, 3 * day);
        busy_job.attach_command = Some("claude attach 7".into());
        let mut gone = session("gone", 6, Status::Idle, 3 * day);
        gone.live = false;
        let mut hosted = session("hosted", 0, Status::Idle, 3 * day);
        (hosted.hosted, hosted.pid) = (true, None);
        let mut pair_new = session("pair-new", 8, Status::Idle, 2 * day);
        pair_new.title = "Newer".into();
        let b = board(vec![
            session("old", 1, Status::Idle, 4 * day),
            session("shared-old", 2, Status::Idle, 3 * day),
            session("shared-busy", 2, Status::Working, 3 * day),
            session("tmux", 3, Status::Idle, 3 * day),
            app,
            job,
            busy_job,
            gone,
            hosted,
            session("pair-old", 8, Status::Idle, 5 * day),
            pair_new,
        ]);
        let p = plan(&b, NOW, day, false, |pid| pid != 3);
        let close: Vec<(&str, Ends, &[String])> = p.close.iter().map(|c| (c.key.as_str(), c.ends, c.with.as_slice())).collect();
        // Longest idle first; a terminal of two closes as one, named by the newer.
        assert_eq!(close, [("old", Ends::Terminal, &[][..]), ("job", Ends::Job, &[][..]), ("pair-new", Ends::Terminal, &["pair-old".to_string()][..])]);
        assert_eq!(p.close[0].pid, Some(1));
        assert_eq!(p.close[1].pid, None);
        let kept: Vec<(&str, &str)> = p.kept.iter().map(|k| (k.key.as_str(), k.why.as_str())).collect();
        assert_eq!(
            kept,
            [
                ("app", "in the Claude app"),
                ("busy-job", "working"),
                ("hosted", "in Codealong"),
                ("shared-old", "working"),
                ("shared-busy", "working"),
                ("tmux", "in a terminal Codealong can't close"),
            ]
        );
    }

    #[test]
    fn archiving_takes_what_cannot_close_too() {
        let day = DAY_MS;
        let mut app = session("app", 4, Status::Unread, 3 * day);
        app.surface = Surface::App;
        let mut asks = session("app-asks", 5, Status::Idle, 3 * day);
        asks.surface = Surface::App;
        asks.reply_tail = Some("Shall I merge it?".into());
        let mut hosted = session("hosted", 0, Status::Idle, 3 * day);
        (hosted.hosted, hosted.pid) = (true, None);
        let mut held_by = session("shared-busy", 2, Status::Working, 3 * day);
        held_by.title = "Fix login".into();
        let b = board(vec![session("old", 1, Status::Idle, 3 * day), session("shared-old", 2, Status::Idle, 3 * day), held_by, session("tmux", 3, Status::Idle, 3 * day), app, asks, hosted]);
        let p = plan(&b, NOW, day, true, |pid| pid != 3);
        assert!(p.archive);
        let mut close: Vec<(&str, Ends, &str)> = p.close.iter().map(|c| (c.key.as_str(), c.ends, c.why.as_str())).collect();
        close.sort_by_key(|c| c.0);
        assert_eq!(
            close,
            [
                ("app", Ends::Archive, "idle 3 days · last reply unread · last reply asks nothing · stays open in the Claude app"),
                ("hosted", Ends::Archive, "idle 3 days · last reply asks nothing · stays open in Codealong"),
                ("old", Ends::Terminal, "idle 3 days · last reply asks nothing"),
                ("shared-old", Ends::Archive, "idle 3 days · last reply asks nothing · its terminal stays open for “Fix login”"),
                ("tmux", Ends::Archive, "idle 3 days · last reply asks nothing · stays open in a terminal Codealong can't close"),
            ]
        );
        // What may not close isn't archived either.
        let kept: Vec<(&str, &str)> = p.kept.iter().map(|k| (k.key.as_str(), k.why.as_str())).collect();
        assert_eq!(kept, [("app-asks", "last reply asks a question"), ("shared-busy", "working")]);
    }

    #[test]
    fn outcomes_say_what_was_done() {
        let ok = Ok(());
        let failed = Err("the tab is gone".to_string());
        let said = |ends, ended, asked, archived| {
            let o = outcome("k", "T".into(), ends, ended, asked, archived);
            (o.closed, o.archived, o.ok, o.message)
        };
        assert_eq!(said(Ends::Terminal, Some(&ok), false, None), (true, false, true, "Closed.".into()));
        assert_eq!(said(Ends::Job, Some(&ok), true, Some(&ok)), (true, true, true, "Stopped and archived.".into()));
        assert_eq!(said(Ends::Terminal, Some(&ok), true, Some(&failed)), (true, false, false, "Closed, but it couldn't be archived: the tab is gone".into()));
        assert_eq!(said(Ends::Terminal, Some(&failed), true, None), (false, false, false, "the tab is gone".into()));
        assert_eq!(said(Ends::Archive, None, true, Some(&ok)), (false, true, true, "Archived; it stays open where it is.".into()));
        assert_eq!(said(Ends::Archive, None, true, Some(&failed)), (false, false, false, "It couldn't be archived: the tab is gone".into()));
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

    #[test]
    fn done_exits_a_session_that_holds_its_own_worktree_its_harness_way() {
        let mut own = session("own", 1, Status::Unread, 0);
        own.own_worktree = Some("/work/.claude/worktrees/brave-otter".into());
        let b = board(vec![own.clone()]);
        assert_eq!(crate::fixtures::session("x").harness, Harness::Claude, "Claude Code has an exit command");
        assert_eq!(ending(&b, &own, false), Ok(Ending::Exit(1)));
        // One that only asks you something in its reply takes the command like a prompt.
        let asks = Session { status: Status::Needs, asks: true, ..own.clone() };
        assert_eq!(ending(&b, &asks, false), Ok(Ending::Exit(1)));
        // Where the command would be taken for something else, it hangs up as before.
        let prompt = Session { status: Status::Needs, ..own.clone() };
        assert_eq!(ending(&b, &prompt, false), Ok(Ending::Terminal(1)));
        let busy = Session { status: Status::Working, ..own.clone() };
        assert_eq!(ending(&b, &busy, true), Ok(Ending::Terminal(1)));
        let stuck = Session { status: Status::Needs, stalled_since: Some(1), ..own.clone() };
        assert_eq!(ending(&b, &stuck, true), Ok(Ending::Terminal(1)));
        let left = Session { background: Some("npm run dev".into()), ..own.clone() };
        assert_eq!(ending(&b, &left, true), Ok(Ending::Terminal(1)));
        // A harness with no exit command of its own hangs up.
        let codex = Session { harness: Harness::Codex, ..own.clone() };
        assert_eq!(ending(&board(vec![codex.clone()]), &codex, false), Ok(Ending::Terminal(1)));
    }

    /// What would close or be archived on this Mac now, doing nothing:
    /// `cargo test this_macs_idle_sessions -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn this_macs_idle_sessions() {
        let p = plan_now(&crate::engine::light_snapshot(), DAY_MS, true);
        for c in &p.close {
            println!("{:<9} {:<10} {:<40} {}", format!("{:?}", c.ends).to_lowercase(), c.account, c.title.chars().take(40).collect::<String>(), c.why);
        }
        for k in &p.kept {
            println!("keep      {:<10} {:<40} {}", k.account, k.title.chars().take(40).collect::<String>(), k.why);
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
