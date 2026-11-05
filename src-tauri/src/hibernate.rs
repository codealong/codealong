//! Hibernating sessions that sat idle a while (`[sessions] hibernate_after`, in minutes; 0, the
//! default, is off): Close idle sessions (idle.rs) done by itself, for sessions in a terminal.
//! Idle sessions add up: each holds its agent's process and what that keeps in memory. A session
//! that hibernates has its terminal tab closed, which ends its agent's process, as Close idle
//! sessions closes one, and isn't archived: it stays where it's listed, and a click on it, or a
//! reply from its box, resumes it in a new tab (`s|<key>|resume`), as for any session whose
//! terminal closed.
//!
//! It goes by Close idle sessions' rules (`idle::plan`: idle or unread, quiet for the whole wait,
//! nothing left running in the background, a last reply that asks nothing, and a terminal only
//! when every session in it can close), and leaves a session alone besides when:
//! - a click couldn't bring it back: its harness can't resume it in a terminal, or its account is
//!   turned off;
//! - anything was typed in its terminal since its turn ended, which may be a prompt you're
//!   writing in its box, and would go with it. What's typed is read by the agent, which moves the
//!   terminal device's access time (`typed_at`). Its modification time is no help: idle Claude
//!   Code sessions write to their terminals every few minutes (2.1.318, 2026-11-05: every one
//!   here had within the half hour, some idle for a day). When that can't be told, it stays;
//! - a session it started is still at work, or waits on you, which it may be waiting to hear back
//!   from;
//! - it's an automatic review's, which reviewdone.rs ends;
//! - its tab is the one you're in, which closing leaves (`launch::close_session_terminals`).
//!
//! Claude Code's background jobs are left alone: Close idle sessions stops one. Its background
//! service retires a job left idle for an hour, but not, as far as was seen here, one a tab is
//! attached to (`claude attach`): jobs attached for a day and more ran on (2.1.317, 2026-11-05).

use std::collections::HashMap;

use serde::Serialize;

use crate::config::Config;
use crate::engine::Handle;
use crate::idle::{Ends, Idle, Kept};
use crate::model::{Board, OriginHow, Session, Status};

/// How long after it was tried a session that didn't hibernate is tried again: one typed in, or
/// whose tab couldn't be found or closed, isn't asked about at every board.
const RETRY_MS: i64 = 30 * 60_000;

/// How often the board is looked at for what may hibernate: the wait is minutes at least.
const LOOK_EVERY_MS: i64 = 60_000;

/// Keys read just before a turn's end was written down count as before it: the answer or the
/// Escape that ended it.
const TYPED_SLACK_MS: i64 = 2_000;

/// The wait, in ms; 0 when it's off.
pub fn after_ms(cfg: &Config) -> i64 {
    i64::from(cfg.sessions.hibernate_after) * 60_000
}

/// What would hibernate, and what stays and why.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub after_ms: i64,
    /// Each terminal that would close, named by the newest session it shows (`idle::plan`).
    pub hibernate: Vec<Idle>,
    pub kept: Vec<Kept>,
}

/// What the plan asks of this Mac, so its rules are tested without one.
pub struct Looks<'a> {
    /// Whether a process runs in a tab Codealong can close (`idle::plan`'s `closable`).
    pub closable: &'a dyn Fn(u32) -> bool,
    /// When something was last typed in the terminal a process runs in, None when that can't be
    /// told. None itself when it isn't asked: the look `Hibernator::due` takes at a board, which
    /// leaves it to the look before closing (`now`).
    pub typed: Option<&'a dyn Fn(u32) -> Option<i64>>,
}

/// What may hibernate on `board` at `now` after `after_ms` idle.
pub fn plan(board: &Board, cfg: &Config, now: i64, after_ms: i64, looks: &Looks) -> Plan {
    let idle = crate::idle::plan(board, now, after_ms, false, looks.closable);
    let mut out = Plan { after_ms, hibernate: Vec::new(), kept: idle.kept };
    for c in idle.close {
        let why = match c.ends {
            Ends::Terminal => keeps(board, cfg, &c, looks),
            // Not archiving, the rest are jobs.
            Ends::Job | Ends::Archive => Some("a background job, which only Close idle sessions stops".into()),
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
    for key in std::iter::once(&c.key).chain(&c.with) {
        let Some((_, s)) = board.session(key) else { return Some("it just changed".into()) };
        if let Some(why) = keeps_session(board, cfg, s) {
            return Some(why);
        }
    }
    // Not asked at a board's look: the look before closing asks.
    let typed = looks.typed?;
    match c.pid.and_then(typed) {
        None => Some("couldn't tell when its terminal was last typed in".into()),
        Some(at) if at > c.last_activity + TYPED_SLACK_MS => Some("something was typed in its terminal since its turn ended".into()),
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

/// What's been tried, so the server hibernates each session once, a batch at a time.
#[derive(Default)]
pub struct Hibernator {
    /// When each session was last handed out, kept for `RETRY_MS`.
    tried: HashMap<String, i64>,
    /// A batch is under way.
    running: bool,
    /// When the board is next looked at.
    next_look: i64,
}

impl Hibernator {
    /// What's due to hibernate at `now` (ms), by the board alone, each marked tried: the sessions
    /// naming the terminals that may close, which `now` looks at again with this Mac's answers.
    /// Nothing while it's off, while a batch is under way, or within a minute of the last look.
    pub fn due(&mut self, board: &Board, cfg: &Config, now: i64) -> Vec<String> {
        let after_ms = after_ms(cfg);
        if after_ms == 0 || self.running || now < self.next_look {
            return Vec::new();
        }
        self.next_look = now + LOOK_EVERY_MS;
        self.tried.retain(|_, at| now - *at < RETRY_MS);
        let looks = Looks { closable: &|_| true, typed: None };
        let due: Vec<String> = plan(board, cfg, now, after_ms, &looks).hibernate.into_iter().map(|c| c.key).filter(|k| !self.tried.contains_key(k)).collect();
        if !due.is_empty() {
            self.running = true;
            self.tried.extend(due.iter().map(|k| (k.clone(), now)));
        }
        due
    }

    /// The batch `due` handed out is over, however it went.
    pub fn ended(&mut self) {
        self.running = false;
    }
}

/// Hibernates the sessions of `keys` (`Hibernator::due`) that may, looked at again now with this
/// Mac's answers: which tabs Codealong can close, and when each was last typed in. What it did
/// goes in the server's log.
pub fn now(engine: &Handle, keys: &[String]) {
    let cfg = match crate::config::current() {
        Ok(cfg) => cfg,
        Err(e) => return eprintln!("Couldn't read config.toml to hibernate idle sessions: {e:#}"),
    };
    let after_ms = after_ms(&cfg);
    if after_ms == 0 {
        return;
    }
    let board = engine.board();
    let fresh = plan_now(&board, &cfg, after_ms);
    let (go, stay): (Vec<&String>, Vec<&String>) = keys.iter().partition(|k| fresh.hibernate.iter().any(|c| &c.key == *k));
    for key in stay {
        if let Some(k) = fresh.kept.iter().find(|k| &k.key == key) {
            eprintln!("Not hibernating “{}”: {}.", k.title, k.why);
        }
    }
    let go: Vec<String> = go.into_iter().cloned().collect();
    if go.is_empty() {
        return;
    }
    for o in crate::idle::close(&board, &cfg, &go, after_ms, None) {
        match o.closed {
            true => eprintln!("Hibernated “{}”, idle {}: its terminal closed, and a click resumes it.", o.title, crate::idle::span(crate::util::now_ms() - board.session(&o.key).map_or(0, |(_, s)| s.last_activity))),
            false => eprintln!("Couldn't hibernate “{}”: {}", o.title, o.message),
        }
    }
}

/// The plan for the board as it is now, with this Mac asked about each terminal.
pub fn plan_now(board: &Board, cfg: &Config, after_ms: i64) -> Plan {
    let closable = |pid: u32| crate::launch::terminal_kind(pid).is_some_and(|t| t.closes_tabs());
    let typed = |pid: u32| typed_at(pid);
    plan(board, cfg, crate::util::now_ms(), after_ms, &Looks { closable: &closable, typed: Some(&typed) })
}

/// When something was last typed in the terminal `pid` runs in: its device's access time, which
/// moves as the program there reads what was typed. None for a process with no terminal.
fn typed_at(pid: u32) -> Option<i64> {
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::metadata(crate::procs::tty(pid)?).ok()?;
    Some(m.atime() * 1000 + m.atime_nsec() / 1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AccountCfg;
    use crate::model::{Helper, SessionOrigin};
    use crate::util::DAY_MS;

    const NOW: i64 = 100 * DAY_MS;
    const HOUR: i64 = 3_600_000;

    fn session(key: &str, pid: u32, quiet_ms: i64) -> Session {
        Session {
            status: Status::Idle,
            pid: Some(pid),
            title: format!("Session {key}"),
            cwd: "/work".into(),
            reply_tail: Some("Pushed it.".into()),
            last_activity: NOW - quiet_ms,
            resume_command: Some(format!("claude --resume {key}")),
            ..crate::fixtures::session(key)
        }
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

    /// The plan with every tab closable and `typed` as each terminal's last keys.
    fn planned(b: &Board, cfg: &Config, typed: &dyn Fn(u32) -> Option<i64>) -> (Vec<String>, Vec<(String, String)>) {
        let p = plan(b, cfg, NOW, after_ms(cfg), &Looks { closable: &|_| true, typed: Some(typed) });
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
    fn what_was_typed_since_its_turn_ended_keeps_it() {
        let c = cfg(60);
        let b = board(vec![session("a", 1, 3 * HOUR)]);
        let ended = NOW - 3 * HOUR;
        // Typed to start the turn, or the key that ended it: before, as far as it goes.
        assert_eq!(planned(&b, &c, &|_| Some(ended - HOUR)).0, ["a"]);
        assert_eq!(planned(&b, &c, &|_| Some(ended + TYPED_SLACK_MS)).0, ["a"]);
        // A prompt being written in its box since, or no telling, keeps it.
        let (go, kept) = planned(&b, &c, &|_| Some(ended + 5 * 60_000));
        assert!(go.is_empty());
        assert_eq!(kept[0].1, "something was typed in its terminal since its turn ended");
        assert_eq!(planned(&b, &c, &|_| None).1[0].1, "couldn't tell when its terminal was last typed in");
        // The look at a board doesn't ask, leaving that to the look before closing.
        assert_eq!(plan(&b, &c, NOW, after_ms(&c), &Looks { closable: &|_| true, typed: None }).hibernate.len(), 1);
    }

    #[test]
    fn stays_awake_for_what_it_started_and_what_a_click_couldnt_bring_back() {
        let c = cfg(60);
        let idle = || Some(NOW - DAY_MS);
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
    fn leaves_background_jobs_to_their_harness() {
        let mut job = session("job", 1, 3 * HOUR);
        job.attach_command = Some("claude attach 1".into());
        let (go, kept) = planned(&board(vec![job]), &cfg(60), &|_| Some(0));
        assert!(go.is_empty());
        assert!(kept[0].1.starts_with("a background job"), "{kept:?}");
    }

    #[test]
    fn the_server_tries_each_once_a_batch_at_a_time() {
        let b = board(vec![session("a", 1, 3 * HOUR), session("b", 2, 3 * HOUR)]);
        // Off by default.
        assert!(Hibernator::default().due(&b, &Config::default(), NOW).is_empty());
        let c = cfg(60);
        let mut h = Hibernator::default();
        let mut due = h.due(&b, &c, NOW);
        due.sort();
        assert_eq!(due, ["a", "b"]);
        // Nothing while the batch runs, nor within the minute after.
        assert!(h.due(&b, &c, NOW + 2 * LOOK_EVERY_MS).is_empty());
        h.ended();
        assert!(h.due(&b, &c, NOW + 3 * LOOK_EVERY_MS).is_empty(), "each was tried already");
        let later = NOW + RETRY_MS + LOOK_EVERY_MS;
        assert_eq!(h.due(&b, &c, later).len(), 2, "tried again once a while has gone by");
        h.ended();
        assert!(h.due(&b, &c, later + 1).is_empty(), "looked at once a minute");
    }

    /// What would hibernate on this Mac now after two hours, doing nothing:
    /// `cargo test this_macs_hibernating_sessions -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn this_macs_hibernating_sessions() {
        let p = plan_now(&crate::engine::light_snapshot(), &crate::config::current().unwrap(), 2 * HOUR);
        for c in &p.hibernate {
            println!("sleep  {:<10} {:<40} {}", c.account, c.title.chars().take(40).collect::<String>(), c.why);
        }
        for k in &p.kept {
            println!("keep   {:<10} {:<40} {}", k.account, k.title.chars().take(40).collect::<String>(), k.why);
        }
    }
}
