//! Hibernating sessions that sat idle a while (`[sessions] hibernate_after`, in minutes; 0, the
//! default, is off): Close idle sessions (idle.rs) done by itself, for sessions in a terminal and
//! Claude Code's background jobs. Idle sessions add up: each holds its agent's process and what
//! that keeps in memory. A session in a terminal that hibernates has its tab closed, which ends its
//! agent's process, as Close idle sessions closes one; a background job is stopped by its harness
//! (`Adapter::stop`), the tabs attached to it (`claude attach`) closed first. Neither is archived:
//! it stays where it's listed, and a click on it, or a reply from its box, resumes it in a new tab
//! (`s|<key>|resume`), as for any session that ended.
//!
//! It goes by Close idle sessions' rules (`idle::plan`: idle or unread, quiet for the whole wait,
//! nothing left running in the background, a last reply that asks nothing, and a terminal only
//! when every session in it can close), and leaves a session alone besides when:
//! - a click couldn't bring it back: its harness can't resume it in a terminal, or its account is
//!   turned off;
//! - anything was typed in its terminal since the last prompt you sent it: a prompt you're writing
//!   in its box, begun while it worked or after, would go with it. That keeps one whose turn you
//!   answered a question in too, until its next prompt. A background job's terminal is its
//!   worker's, which the tabs attached to it type into. What's typed is read by the agent, which
//!   moves the terminal device's access time; its modification time is no help, as idle Claude
//!   Code sessions write to their terminals every few minutes (2.1.318, 2026-11-05: every one here
//!   had within the half hour, some idle for a day). But a resize has every program reread its
//!   terminal at once, the agents' too, so the time is taken as the engine's looks saw it, a burst
//!   of them not counted (`stale::Uses::typed`). When it can't be told, it stays;
//! - a session it started is still at work, or waits on you, which it may be waiting to hear back
//!   from;
//! - it's an automatic review's, which reviewdone.rs ends;
//! - its terminal, or one attached to the job, isn't one whose closing leaves the tab you're in
//!   (`Terminal::spares_yours`): Ghostty, iTerm2, Terminal and Rex are; tmux panes and Codealong's
//!   own terminals aren't. Nor does it close one while it can't be told which tab you're in
//!   (`launch::Closing::unasked`), and a job one of whose tabs stays keeps running.
//!
//! The server looks once a minute on a thread of its own (`spawn`), not as the board changes: a
//! board that sits still is what a session crossing the wait looks like. Each session is looked at
//! again, board and keys, before its tab is looked for, and once more after it's found, just
//! before its agent is hung up or its job stopped (`asked_again`): finding a tab can wait its turn
//! behind a send.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use serde::Serialize;

use crate::config::Config;
use crate::engine::Handle;
use crate::idle::{Ends, Idle, Kept};
use crate::model::{Board, OriginHow, Session, Status};
use crate::procs::PsLine;
use crate::stale::Uses;

/// How long after it was tried a session that didn't hibernate is tried again: one typed in, or
/// whose tab couldn't be found or closed, isn't asked about at every look.
const RETRY_MS: i64 = 30 * 60_000;

/// How often the server looks for what may hibernate: the wait is minutes at least.
const LOOK_EVERY: Duration = Duration::from_secs(60);

/// The wait, in ms; 0 when it's off.
pub fn after_ms(cfg: &Config) -> i64 {
    i64::from(cfg.sessions.hibernate_after) * 60_000
}

/// What would hibernate, and what stays and why.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub after_ms: i64,
    /// Each terminal that would close, named by the newest session it shows, and each job that
    /// would stop (`idle::plan`).
    pub hibernate: Vec<Idle>,
    pub kept: Vec<Kept>,
}

/// What the plan asks of this Mac, so its rules are tested without one.
pub struct Looks<'a> {
    /// Whether a process runs in a tab that may close with nobody asking (`idle::plan`'s
    /// `closable`).
    pub closable: &'a dyn Fn(u32) -> bool,
    /// When something was last typed in the terminal a process runs in, None when that can't be
    /// told. None itself when it isn't asked: `Hibernator::due`'s look, which leaves it to the
    /// look just before closing (`one`).
    pub typed: Option<&'a dyn Fn(u32) -> Option<i64>>,
    /// The processes showing a background job in a tab, by the job's attach command
    /// (`Session::attach_command`): each a tab to close before the job stops.
    pub attached: &'a dyn Fn(&str) -> Vec<u32>,
}

/// What may hibernate on `board` at `now` after `after_ms` idle.
pub fn plan(board: &Board, cfg: &Config, now: i64, after_ms: i64, looks: &Looks) -> Plan {
    let idle = crate::idle::plan(board, now, after_ms, false, looks.closable);
    let mut out = Plan { after_ms, hibernate: Vec::new(), kept: idle.kept };
    for c in idle.close {
        let why = match c.ends {
            Ends::Terminal => keeps(board, cfg, &c, looks),
            Ends::Job => keeps_job(board, cfg, &c, looks),
            // Not archiving, nothing is listed to be archived only.
            Ends::Archive => Some("it would only be archived".into()),
        };
        match why {
            Some(why) => out.kept.push(Kept { key: c.key, title: c.title, account: c.account, why }),
            None => out.hibernate.push(c),
        }
    }
    out
}

/// Why the terminal `c` stays open, though Close idle sessions would close it; None when it may
/// hibernate.
fn keeps(board: &Board, cfg: &Config, c: &Idle, looks: &Looks) -> Option<String> {
    let mut prompted = 0;
    for key in std::iter::once(&c.key).chain(&c.with) {
        let Some((_, s)) = board.session(key) else { return Some("it just changed".into()) };
        if let Some(why) = keeps_session(board, cfg, s) {
            return Some(why);
        }
        prompted = prompted.max(s.last_prompt_at);
    }
    typed_since(looks, c.pid, prompted)
}

/// Why the background job `c` keeps running, though Close idle sessions would stop it; None when
/// it may hibernate. Its terminal is its worker's (`Session::pid`), and a tab attached to it must
/// be one that may close.
fn keeps_job(board: &Board, cfg: &Config, c: &Idle, looks: &Looks) -> Option<String> {
    let Some((_, s)) = board.session(&c.key) else { return Some("it just changed".into()) };
    if let Some(why) = keeps_session(board, cfg, s) {
        return Some(why);
    }
    if (looks.attached)(s.attach_command.as_deref().unwrap_or_default()).into_iter().any(|pid| !(looks.closable)(pid)) {
        return Some("a terminal attached to it can't say whether it's the tab you're in".into());
    }
    typed_since(looks, s.pid, s.last_prompt_at)
}

/// Why what was typed in the terminal of `pid` keeps it: anything since `prompted`, or no telling.
/// None when it may go, and when it isn't asked (`Hibernator::due`'s look: the look before
/// closing asks).
fn typed_since(looks: &Looks, pid: Option<u32>, prompted: i64) -> Option<String> {
    let typed = looks.typed?;
    match pid.and_then(typed) {
        None => Some("couldn't tell when its terminal was last typed in".into()),
        // The prompt is written down once its Return is read, so keys read after it aren't its own.
        Some(at) if at > prompted => Some("something was typed in its terminal since its last prompt".into()),
        Some(_) => None,
    }
}

/// Why `s` itself stays awake, whatever its terminal says.
fn keeps_session(board: &Board, cfg: &Config, s: &Session) -> Option<String> {
    if s.helper.as_ref().is_some_and(|h| h.pr.is_some()) {
        return Some("an automatic review's session".into());
    }
    let helping = board.lanes.iter().flat_map(|l| &l.sessions).find(|o| o.live && o.key != s.key && matches!(o.status, Status::Working | Status::Needs) && started_by(o, &s.key));
    if let Some(o) = helping {
        return Some(format!("“{}”, a session it started, is still at it", o.title));
    }
    let Some(account) = cfg.accounts.iter().find(|a| a.name == s.account) else { return Some(format!("there's no account named {} in config.toml", s.account)) };
    if account.disabled {
        return Some("its account is turned off".into());
    }
    if crate::harness::adapter(s.harness).resume_typed(account, s).is_none() {
        return Some("its harness can't resume it in a terminal".into());
    }
    None
}

/// Whether the session `key`'s agent started `o`.
fn started_by(o: &Session, key: &str) -> bool {
    o.helper.as_ref().and_then(|h| h.parent.as_deref()) == Some(key) || o.origin.as_ref().is_some_and(|g| g.how == OriginHow::Started && g.key == key)
}

/// What's been tried, so the server tries each session once in a while.
#[derive(Default)]
pub struct Hibernator {
    /// When each session was last handed out, kept for `RETRY_MS`.
    tried: HashMap<String, i64>,
}

impl Hibernator {
    /// What's due to hibernate at `now` (ms), by the board alone, each marked tried: the sessions
    /// naming the terminals that may close and the jobs that may stop, which `one` looks at again
    /// with this Mac's answers. Nothing while it's off, or while the board is still loading.
    pub fn due(&mut self, board: &Board, cfg: &Config, now: i64) -> Vec<String> {
        let after_ms = after_ms(cfg);
        if after_ms == 0 || board.loading {
            return Vec::new();
        }
        self.tried.retain(|_, at| now - *at < RETRY_MS);
        let looks = Looks { closable: &|_| true, typed: None, attached: &|_| Vec::new() };
        let due: Vec<String> = plan(board, cfg, now, after_ms, &looks).hibernate.into_iter().map(|c| c.key).filter(|k| !self.tried.contains_key(k)).collect();
        self.tried.extend(due.iter().map(|k| (k.clone(), now)));
        due
    }
}

/// Looks once a minute, for as long as the server runs, and hibernates what's due, one session
/// at a time. What it did goes in the server's log.
pub fn spawn(engine: Handle) {
    let started = std::thread::Builder::new().name("codealong-hibernate".into()).spawn(move || {
        let mut hibernator = Hibernator::default();
        loop {
            std::thread::sleep(LOOK_EVERY);
            let Ok(cfg) = crate::config::current() else { continue };
            for key in hibernator.due(&engine.board(), &cfg, crate::util::now_ms()) {
                // One that goes wrong doesn't stop the looks.
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| one(&engine, &cfg, &key))).is_err() {
                    eprintln!("Hibernating {key} stopped partway.");
                }
            }
        }
    });
    if let Err(e) = started {
        eprintln!("codealong: idle sessions won't hibernate: {e}");
    }
}

/// Hibernates the session `key` if it still may, with the board as it is now and this Mac asked
/// about it now: which tabs close with nobody asking, when it was last typed in, and what's
/// attached to a job.
fn one(engine: &Handle, cfg: &Config, key: &str) {
    let board = engine.board();
    let fresh = plan_now(&board, cfg, after_ms(cfg), engine.terminal_use());
    let Some(c) = fresh.hibernate.iter().find(|c| c.key == key) else {
        if let Some(k) = fresh.kept.iter().find(|k| k.key == key) {
            eprintln!("Not hibernating “{}”: {}.", k.title, k.why);
        }
        return;
    };
    let idle_for = crate::idle::span(crate::util::now_ms() - c.last_activity);
    let done = match c.ends {
        Ends::Terminal => close_terminal(engine, c),
        Ends::Job => stop_job(engine, &board, c),
        Ends::Archive => return,
    };
    match done {
        Ok(what) => eprintln!("Hibernated “{}”, idle {idle_for}: {what}, and a click resumes it.", c.title),
        Err(why) => eprintln!("Couldn't hibernate “{}”: {why}", c.title),
    }
}

/// Closes the terminal `c` names, asked once more just before its agent is hung up.
fn close_terminal(engine: &Handle, c: &Idle) -> Result<&'static str, String> {
    let pid = c.pid.ok_or("it has no terminal")?;
    let only = |p: u32| p == pid;
    let check = || asked_again(engine, &c.key, Some(pid), &only);
    let closing = crate::launch::Closing { pid, cwd: Path::new(&c.cwd), hang_up: vec![pid], unasked: Some(&check) };
    match crate::launch::close_terminals(&[closing]).pop() {
        Some(Ok(())) => Ok("its terminal closed"),
        Some(Err(e)) => Err(format!("{e:#}")),
        None => Err("nothing was closed".into()),
    }
}

/// Stops the background job `c`: the tabs attached to it closed first, each as a tab nobody asked
/// to close, so the one you're in, or one whose terminal can't say, keeps the job running; then
/// the job, asked once more, with no tab attached to it since.
fn stop_job(engine: &Handle, board: &Board, c: &Idle) -> Result<&'static str, String> {
    let (_, s) = board.session(&c.key).ok_or("it isn't on the board any more")?;
    let tabs = attached_now(s.attach_command.as_deref().unwrap_or_default());
    if !tabs.is_empty() {
        let shown = |p: u32| tabs.contains(&p);
        let check = || asked_again(engine, &c.key, None, &shown);
        let closing: Vec<crate::launch::Closing> = tabs.iter().map(|&pid| crate::launch::Closing { pid, cwd: Path::new(&s.cwd), hang_up: vec![pid], unasked: Some(&check) }).collect();
        if let Some(Err(e)) = crate::launch::close_terminals(&closing).into_iter().find(|r| r.is_err()) {
            return Err(format!("a tab attached to it stays: {e:#}"));
        }
    }
    if !asked_again(engine, &c.key, None, &|_| false) {
        return Err("it changed, or a tab was attached to it, as it was about to stop".into());
    }
    let cfg = crate::config::current().map_err(crate::util::words)?;
    let board = engine.board();
    let (s, account) = crate::harness::find(&board, &cfg, &c.key).map_err(crate::util::words)?;
    crate::harness::adapter(s.harness).stop(&crate::harness::Ctx { account }, s).map_err(crate::util::words)?;
    Ok("its job stopped")
}

/// Whether `key` may still hibernate, asked of the board, the settings and this Mac as they are
/// now, just before its agent is hung up or its job stopped: the plan's own rules, with
/// `closable` the tabs about to close, and it named as it was (`pid`, its terminal's process; None
/// for a job).
fn asked_again(engine: &Handle, key: &str, pid: Option<u32>, closable: &dyn Fn(u32) -> bool) -> bool {
    let Ok(cfg) = crate::config::current() else { return false };
    let uses = engine.terminal_use();
    let typed = |p: u32| typed_at(&uses, p);
    still(&engine.board(), &cfg, crate::util::now_ms(), key, pid, &Looks { closable, typed: Some(&typed), attached: &attached_now })
}

/// Whether `key` may still hibernate on `board` and `cfg` as they are now: the plan's own rules,
/// and it named as it was (a session new in its terminal since names it). Not once it's turned off.
fn still(board: &Board, cfg: &Config, now: i64, key: &str, pid: Option<u32>, looks: &Looks) -> bool {
    let after_ms = after_ms(cfg);
    if after_ms == 0 {
        return false;
    }
    plan(board, cfg, now, after_ms, looks).hibernate.iter().any(|c| c.key == key && c.pid == pid)
}

/// The plan for the board as it is now, with this Mac asked about each terminal and job, and what
/// was typed where as the engine's looks saw it (`uses`), which this look adds to, as Close stale
/// terminals' does (`stale::tabs_now`).
pub fn plan_now(board: &Board, cfg: &Config, after_ms: i64, mut uses: Uses) -> Plan {
    let listed = crate::procs::list(Duration::from_secs(10));
    uses.on_read_look(crate::util::now_ms(), &crate::stale::device_read_times(&listed.lines));
    let closable = |pid: u32| crate::launch::terminal_kind(pid).is_some_and(|t| t.closes_tabs() && t.spares_yours());
    let typed = |pid: u32| typed_at(&uses, pid);
    let attached = |attach: &str| attached_in(&listed.lines, attach);
    plan(board, cfg, crate::util::now_ms(), after_ms, &Looks { closable: &closable, typed: Some(&typed), attached: &attached })
}

/// When something was last typed in the terminal `pid` runs in: when its device was last read
/// from, as the engine's looks saw it, a burst of rereads not counted (`Uses::typed`). None for a
/// process with no terminal.
fn typed_at(uses: &Uses, pid: u32) -> Option<i64> {
    let device = crate::procs::tty(pid)?;
    let time = crate::stale::last_read(&device)?;
    Some(uses.typed(device.strip_prefix("/dev").ok()?.to_str()?, time))
}

/// The processes running `attach` (a background job's attach command) now.
fn attached_now(attach: &str) -> Vec<u32> {
    attached_in(&crate::procs::list(Duration::from_secs(10)).lines, attach)
}

/// The processes in `lines` that run `attach`: by its arguments (`attach <job>`), since its
/// program may be named by a path, a wrapper or settings before it.
fn attached_in(lines: &[PsLine], attach: &str) -> Vec<u32> {
    let args: Vec<&str> = attach.split_whitespace().skip_while(|w| w.contains('=')).skip(1).collect();
    if args.is_empty() {
        return Vec::new();
    }
    lines
        .iter()
        .filter(|l| {
            let words: Vec<&str> = l.command.split_whitespace().collect();
            words.len() > args.len() && words.ends_with(&args)
        })
        .map(|l| l.pid)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AccountCfg;
    use crate::model::{Helper, SessionOrigin};
    use crate::util::DAY_MS;

    const NOW: i64 = 100 * DAY_MS;
    const HOUR: i64 = 3_600_000;
    /// How long each session's last turn took, from its prompt to its last activity.
    const TURN: i64 = 10 * 60_000;

    fn session(key: &str, pid: u32, quiet_ms: i64) -> Session {
        Session {
            status: Status::Idle,
            pid: Some(pid),
            title: format!("Session {key}"),
            cwd: "/work".into(),
            reply_tail: Some("Pushed it.".into()),
            last_activity: NOW - quiet_ms,
            last_prompt_at: NOW - quiet_ms - TURN,
            resume_command: Some(format!("claude --resume {key}")),
            ..crate::fixtures::session(key)
        }
    }

    /// A background job whose worker is `pid`.
    fn job(key: &str, pid: u32, quiet_ms: i64) -> Session {
        Session { attach_command: Some(format!("claude attach {key}")), ..session(key, pid, quiet_ms) }
    }

    fn board(sessions: Vec<Session>) -> Board {
        let mut b = Board::default();
        b.lanes = vec![crate::fixtures::lane("/work", sessions)];
        b
    }

    fn cfg(minutes: u32) -> Config {
        let mut cfg = Config::default();
        cfg.accounts = vec![AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false }];
        cfg.sessions.hibernate_after = minutes;
        cfg
    }

    fn none_attached(_: &str) -> Vec<u32> {
        Vec::new()
    }

    /// Typed in long before anything here.
    fn long_ago(_: u32) -> Option<i64> {
        Some(0)
    }

    fn only_one(pid: u32) -> bool {
        pid == 1
    }

    /// The plan with every tab closable, nothing attached, and `typed` as each terminal's last keys.
    fn planned(b: &Board, cfg: &Config, typed: &dyn Fn(u32) -> Option<i64>) -> (Vec<String>, Vec<(String, String)>) {
        let p = plan(b, cfg, NOW, after_ms(cfg), &Looks { closable: &|_| true, typed: Some(typed), attached: &none_attached });
        (p.hibernate.into_iter().map(|c| c.key).collect(), p.kept.into_iter().map(|k| (k.key, k.why)).collect())
    }

    #[test]
    fn hibernates_what_sat_idle_past_the_wait_by_close_idle_sessions_rules() {
        let c = cfg(120);
        let mut asks = session("asks", 3, 5 * HOUR);
        asks.reply_tail = Some("Shall I push it?".into());
        let b = board(vec![session("old", 1, 3 * HOUR), session("recent", 2, HOUR), asks]);
        let (go, kept) = planned(&b, &c, &|_| Some(NOW - DAY_MS));
        assert_eq!(go, ["old"]);
        assert_eq!(kept, [("recent".to_string(), "active an hour ago".to_string()), ("asks".to_string(), "last reply asks a question".to_string())]);
    }

    #[test]
    fn what_was_typed_since_its_last_prompt_keeps_it() {
        let c = cfg(60);
        let b = board(vec![session("a", 1, 3 * HOUR)]);
        let (ended, prompted) = (NOW - 3 * HOUR, NOW - 3 * HOUR - TURN);
        // Typed before the prompt was written down: the prompt itself, its Return last.
        assert_eq!(planned(&b, &c, &|_| Some(prompted - HOUR)).0, ["a"]);
        assert_eq!(planned(&b, &c, &|_| Some(prompted)).0, ["a"]);
        // A prompt being written in its box, begun after its turn or while it worked, even straight
        // after the last one went, keeps it, as does no telling.
        for typed in [ended + 5 * 60_000, ended - 5 * 60_000, ended + 1, prompted + 500] {
            let (go, kept) = planned(&b, &c, &|_| Some(typed));
            assert!(go.is_empty(), "typed {} after the prompt", typed - prompted);
            assert_eq!(kept[0].1, "something was typed in its terminal since its last prompt");
        }
        assert_eq!(planned(&b, &c, &|_| None).1[0].1, "couldn't tell when its terminal was last typed in");
        // The due look doesn't ask, leaving that to the look before closing.
        assert_eq!(plan(&b, &c, NOW, after_ms(&c), &Looks { closable: &|_| true, typed: None, attached: &none_attached }).hibernate.len(), 1);
    }

    #[test]
    fn a_terminal_of_several_sessions_goes_by_the_latest_prompt_among_them() {
        let c = cfg(60);
        let older = session("older", 1, 5 * HOUR);
        // Prompted after the older one's turn, in the same terminal (an OpenCode TUI shows several).
        let newer = session("newer", 1, 3 * HOUR);
        let between = NOW - 5 * HOUR + 60_000;
        assert!(planned(&board(vec![older.clone(), newer.clone()]), &c, &|_| Some(between)).0 == ["newer"], "typed before the newer one's prompt");
        assert!(planned(&board(vec![older, newer]), &c, &|_| Some(NOW - 2 * HOUR)).0.is_empty());
    }

    #[test]
    fn only_terminals_that_spare_the_tab_youre_in_close() {
        let b = board(vec![session("tmux", 7, 3 * HOUR), session("ghostty", 8, 3 * HOUR)]);
        let c = cfg(60);
        let p = plan(&b, &c, NOW, after_ms(&c), &Looks { closable: &|pid| pid == 8, typed: Some(&|_| Some(0)), attached: &none_attached });
        assert_eq!(p.hibernate.iter().map(|h| h.key.as_str()).collect::<Vec<_>>(), ["ghostty"]);
        assert_eq!(p.kept.iter().map(|k| (k.key.as_str(), k.why.as_str())).collect::<Vec<_>>(), [("tmux", "in a terminal Codealong can't close")]);
    }

    #[test]
    fn stays_awake_for_what_it_started_and_what_a_click_couldnt_bring_back() {
        let c = cfg(60);
        let idle = || Some(0);
        // A helper it started that's still at work, or waits on you: it may be waiting to hear back.
        let mut helper = session("helper", 9, 0);
        helper.status = Status::Working;
        helper.helper = Some(Helper { parent: Some("parent".into()), ..Default::default() });
        let b = board(vec![session("parent", 1, 3 * HOUR), helper.clone()]);
        assert_eq!(planned(&b, &c, &|_| idle()).1.iter().find(|k| k.0 == "parent").map(|k| k.1.as_str()), Some("“Session helper”, a session it started, is still at it"));
        let mut started = helper.clone();
        (started.helper, started.origin, started.status) = (None, Some(SessionOrigin { key: "parent".into(), how: OriginHow::Started }), Status::Needs);
        assert!(planned(&board(vec![session("parent", 1, 3 * HOUR), started]), &c, &|_| idle()).0.is_empty());
        // One that's done doesn't; nor does a fork of it, which goes on by itself.
        helper.status = Status::Idle;
        let mut fork = session("fork", 8, 0);
        (fork.status, fork.origin) = (Status::Working, Some(SessionOrigin { key: "parent".into(), how: OriginHow::Fork }));
        assert_eq!(planned(&board(vec![session("parent", 1, 3 * HOUR), helper, fork]), &c, &|_| idle()).0, ["parent"]);

        // An automatic review's session is reviewdone.rs's.
        let mut review = session("review", 1, 3 * HOUR);
        review.helper = Some(Helper { pr: Some("acme/app#7".into()), ..Default::default() });
        assert_eq!(planned(&board(vec![review]), &c, &|_| idle()).1[0].1, "an automatic review's session");
        // Nothing to resume it with, or an account that starts nothing.
        let mut no_resume = session("x", 1, 3 * HOUR);
        no_resume.resume_command = None;
        assert_eq!(planned(&board(vec![no_resume]), &c, &|_| idle()).1[0].1, "its harness can't resume it in a terminal");
        let mut off = cfg(60);
        off.accounts[0].disabled = true;
        assert_eq!(planned(&board(vec![session("x", 1, 3 * HOUR)]), &off, &|_| idle()).1[0].1, "its account is turned off");
    }

    #[test]
    fn a_background_job_goes_by_its_workers_terminal_and_the_tabs_attached_to_it() {
        let c = cfg(60);
        let b = board(vec![job("j1", 5, 3 * HOUR), job("busy", 6, 0)]);
        let prompted = NOW - 3 * HOUR - TURN;
        // Idle, its worker's terminal not typed in since its prompt, nothing attached.
        assert_eq!(planned(&b, &c, &|_| Some(prompted - HOUR)).0, ["j1"]);
        let p = plan(&b, &c, NOW, after_ms(&c), &Looks { closable: &|_| true, typed: Some(&|_| Some(0)), attached: &none_attached });
        assert_eq!((p.hibernate[0].ends, p.hibernate[0].pid), (Ends::Job, None));
        // Typed into through a tab attached to it since: a draft in its box.
        let kept = planned(&b, &c, &|pid| Some(if pid == 5 { prompted + 60_000 } else { 0 })).1;
        assert!(kept.contains(&("j1".to_string(), "something was typed in its terminal since its last prompt".to_string())), "{kept:?}");
        // Its tabs must be ones that may close: in tmux, say, it keeps running.
        let attached = |attach: &str| if attach == "claude attach j1" { vec![70] } else { Vec::new() };
        let with = |closable: &dyn Fn(u32) -> bool| plan(&b, &c, NOW, after_ms(&c), &Looks { closable, typed: Some(&long_ago), attached: &attached });
        assert_eq!(with(&|_| true).hibernate.len(), 1);
        let kept = with(&|pid| pid != 70).kept;
        assert!(kept.iter().any(|k| k.key == "j1" && k.why == "a terminal attached to it can't say whether it's the tab you're in"), "{kept:?}");
        // Just before it stops, no tab may be attached any more: one attached since would see it end.
        assert!(still(&b, &c, NOW, "j1", None, &Looks { closable: &|_| false, typed: Some(&long_ago), attached: &none_attached }));
        assert!(!still(&b, &c, NOW, "j1", None, &Looks { closable: &|_| false, typed: Some(&long_ago), attached: &attached }));
    }

    #[test]
    fn a_jobs_tabs_are_found_by_the_arguments_of_its_attach_command() {
        let line = |pid: u32, command: &str| PsLine { pid, ppid: 1, tty: "ttys001".into(), cpu: None, command: command.into() };
        let lines = [
            line(10, "claude attach 39719994"),
            line(11, "/Users/me/.local/bin/claude attach 39719994"),
            line(12, "claude attach 3971999"),
            line(13, "claude attach 39719994 extra"),
            line(14, "attach 39719994"),
            line(15, "claude2 attach 39719994"),
        ];
        assert_eq!(attached_in(&lines, "claude attach 39719994"), [10, 11, 15]);
        assert_eq!(attached_in(&lines, "CLAUDE_CONFIG_DIR=/Users/me/.claude2 claude attach 39719994"), [10, 11, 15]);
        assert!(attached_in(&lines, "claude").is_empty());
    }

    #[test]
    fn the_server_tries_each_once_in_a_while_as_the_clock_goes() {
        let b = board(vec![session("a", 1, 50 * 60_000), session("b", 2, 3 * HOUR)]);
        // Off by default, and nothing while the first scan is still coming in.
        assert!(Hibernator::default().due(&b, &Config::default(), NOW).is_empty());
        let c = cfg(60);
        let mut loading = b.clone();
        loading.loading = true;
        assert!(Hibernator::default().due(&loading, &c, NOW).is_empty());

        let mut h = Hibernator::default();
        assert_eq!(h.due(&b, &c, NOW), ["b"]);
        assert!(h.due(&b, &c, NOW + 60_000).is_empty(), "b was tried already, and a hasn't sat the hour");
        // The same board, unchanged, once a has sat the hour.
        assert_eq!(h.due(&b, &c, NOW + 11 * 60_000), ["a"]);
        assert_eq!(h.due(&b, &c, NOW + RETRY_MS), ["b"], "tried again once a while has gone by");
    }

    #[test]
    fn its_tab_found_the_same_rules_are_asked_again() {
        let c = cfg(60);
        let at = |b: &Board, typed: &dyn Fn(u32) -> Option<i64>| still(b, &c, NOW, "a", Some(1), &Looks { closable: &only_one, typed: Some(typed), attached: &none_attached });
        let before = &long_ago;
        let mut helper = session("helper", 9, 0);
        helper.helper = Some(Helper { parent: Some("a".into()), ..Default::default() });
        let b = board(vec![session("a", 1, 3 * HOUR), session("b", 1, 4 * HOUR), helper]);
        assert!(at(&b, before));
        // Its helper went back to work meanwhile, with nothing new in it.
        let mut helping = b.clone();
        helping.lanes[0].sessions[2].status = Status::Working;
        assert!(!at(&helping, before));
        // A draft begun since, or no telling, whatever was read before.
        assert!(!at(&b, &|_| Some(NOW - 3 * HOUR - TURN + 500)));
        assert!(!at(&b, &|_| None));
        // A prompt came in, it's gone, or a session new in its terminal names it now.
        let mut moved = b.clone();
        moved.lanes[0].sessions[0].last_activity = NOW;
        assert!(!at(&moved, before));
        let mut gone = b.clone();
        gone.lanes[0].sessions[0].live = false;
        assert!(!at(&gone, before));
        let mut joined = b.clone();
        joined.lanes[0].sessions.push(session("c", 1, 2 * HOUR));
        assert!(!at(&joined, before));
        // The settings changed while its tab was looked for: turned off, a wait it hasn't sat, or
        // its account turned off.
        let looks = Looks { closable: &only_one, typed: Some(&long_ago), attached: &none_attached };
        for changed in [cfg(0), cfg(4 * 60)] {
            assert!(!still(&b, &changed, NOW, "a", Some(1), &looks), "{}", changed.sessions.hibernate_after);
        }
        let mut off = cfg(60);
        off.accounts[0].disabled = true;
        assert!(!still(&b, &off, NOW, "a", Some(1), &looks));
    }

    /// What would hibernate on this Mac now after two hours, doing nothing:
    /// `cargo test this_macs_hibernating_sessions -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn this_macs_hibernating_sessions() {
        let p = plan_now(&crate::engine::light_snapshot(), &crate::config::current().unwrap(), 2 * HOUR, Uses::load());
        for c in &p.hibernate {
            println!("sleep  {:<10} {:<40} {}", c.account, c.title.chars().take(40).collect::<String>(), c.why);
        }
        for k in &p.kept {
            println!("keep   {:<10} {:<40} {}", k.account, k.title.chars().take(40).collect::<String>(), k.why);
        }
    }
}
