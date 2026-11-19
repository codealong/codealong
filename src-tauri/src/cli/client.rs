//! Where a command's answers come from (`Client`): Codealong's server over its socket, or the
//! files when none answers, which give a light board (no git, PRs or visitors). Every handler is
//! handed one.
//!
//! - `board` is the board with what the session running the command may not read of a boundary
//!   group's sessions cut to their metadata (`veil`). The cut is made in the one copy that's
//!   kept, so `raw_board` is that copy as it stands: everything only until `board` has first
//!   been asked for, and cut after. It's for finding which session runs the command (`find_me`),
//!   which the cut waits on, and no way round the cut later. `loaded_board` is the one to look a
//!   name up on: it waits for a server's first scan after a start.
//! - What a command does to a session or a worktree is one of the app's own requests, asked with
//!   `act` (actions.rs): the server does it, or this command over the board when none runs. A
//!   todo change goes through `apply`, a start through `start`.
//! - A command that can't do without the server (`app` is None) says so, rather than answer from
//!   a board that can't keep what it asks for.

use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::Value;

use crate::actions;
use crate::api;
use crate::config::{self, Config};
use crate::engine;
use crate::model::{Board, Lane, Session};
use crate::search;
use crate::server::{self, Request};
use crate::start::{self, Place, StartSpec};
use crate::todos::{self, TodoOp, TodoStore};
use crate::util::now_ms;

use super::*;

/// How long a lookup waits for the app's first scan after a start (`Client::loaded_board`), which
/// took 3 to 7 s at a load of 100 to 300 on 2026-10-22's relaunches, how often it asks, and how
/// long it waits for an answer.
const LOADING_WAIT: std::time::Duration = std::time::Duration::from_secs(30);
const LOADING_EVERY: std::time::Duration = std::time::Duration::from_millis(500);
const LOADING_ASK: std::time::Duration = std::time::Duration::from_secs(5);

/// Waits, at most `within`, for an app still reading after a start to finish, asking with `ask`,
/// which waits no longer than it's given: the app's status once it has, or None when it stops
/// answering or the time is up first.
fn loaded_within(within: std::time::Duration, mut ask: impl FnMut(std::time::Duration) -> Option<server::Status>) -> Option<server::Status> {
    let until = std::time::Instant::now() + within;
    loop {
        std::thread::sleep(LOADING_EVERY.min(until.saturating_duration_since(std::time::Instant::now())));
        // Zero would have the ask wait for as long as it takes (`server::ask_within`).
        let left = until.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return None;
        }
        match ask(left.min(LOADING_ASK))? {
            status if status.loading => {}
            status => return Some(status),
        }
    }
}

/// Where answers come from: Codealong's server over its socket, or the files.
pub(super) struct Client {
    pub(super) app: Option<server::Status>,
    pub(super) verbose: bool,
    pub(super) cfg: Option<Config>,
    pub(super) board: Option<Board>,
    /// The board is the app's, not a light one read from the files.
    pub(super) from_app: bool,
    /// The server was still reading when this command found it, or this command started it: the
    /// first board read waits for its first scan, until when its board has none of the sessions
    /// and worktrees a light one from the files would.
    pub(super) started: bool,
    pub(super) said: bool,
    /// The session running this command and how it was found, once `find_me` has looked.
    pub(super) me: Option<(Option<String>, Option<&'static str>)>,
    /// Your other Macs' worktrees with their sessions, once `elsewhere` has asked the server.
    pub(super) elsewhere: Option<Vec<Lane>>,
    /// The sessions on the board whose text the session running this command may not have
    /// (boundary.rs), once `board` has cut them to their metadata: None until it has.
    pub(super) veiled: Option<HashSet<String>>,
}

impl Client {
    /// With `start`, a server is started when none answers (serve.rs `ensure`), which is said
    /// on stderr with `say`; one that can't be started leaves the files to answer.
    pub(super) fn new(verbose: bool, start: bool, say: bool) -> Client {
        let mut app: Option<server::Status> = match server::ask(&Request::Status) {
            Some(Ok(v)) => serde_json::from_value(v).ok(),
            _ => None,
        };
        let mut started = app.as_ref().is_some_and(|s| s.loading);
        if app.is_none() && start {
            match crate::serve::ensure(false, None) {
                Ok(s) => {
                    if say || verbose {
                        eprintln!("Started Codealong's server (pid {}).", s.pid);
                    }
                    app = Some(s);
                    started = true;
                }
                Err(e) if verbose => eprintln!("{e}"),
                Err(_) => {}
            }
        }
        Client { app, verbose, cfg: None, board: None, from_app: false, started, said: false, me: None, elsewhere: None, veiled: None }
    }

    pub(super) fn note(&mut self) {
        if self.verbose && !self.said {
            self.said = true;
            match &self.app {
                Some(s) => eprintln!("via Codealong's server (pid {}, version {})", s.pid, s.version),
                None => eprintln!("via the files (Codealong's server isn't running)"),
            }
        }
    }

    /// config.toml's settings, or why they can't be had: a file that doesn't parse stops
    /// whatever needs them, rather than have it run on the defaults, which have no accounts.
    pub(super) fn cfg(&mut self) -> Result<&Config> {
        if self.cfg.is_none() {
            self.cfg = Some(config::current()?);
        }
        Ok(self.cfg.as_ref().unwrap())
    }

    /// The board: the app's, or a light one from the files (no git, PRs or visitors), with what
    /// the session running this command may not read of a boundary group's sessions left out
    /// (`veil`).
    pub(super) fn board(&mut self) -> Result<&Board> {
        self.raw_board()?;
        if self.veiled.is_none() {
            self.veil()?;
        }
        Ok(self.board.as_ref().unwrap())
    }

    /// The board as the server or the files have it, everything in it: for finding which
    /// session runs this command, before what it may read is known.
    pub(super) fn raw_board(&mut self) -> Result<&Board> {
        self.note();
        if std::mem::take(&mut self.started) {
            self.wait_loaded(LOADING_WAIT);
        }
        if self.board.is_none() {
            // An app that answered before and doesn't now is restarting: the board is a light one
            // until it's back.
            let (board, from_app) = match self.app.is_some() {
                true => match server::ask(&Request::Board) {
                    Some(Ok(v)) => {
                        let mut board: Board = serde_json::from_value(v).context("reading the app's board")?;
                        // Not sent, but made of what is: how a PR's settings go for its repo.
                        board.repo_settings = crate::teamfile::RepoSettings::sent(&board.repos, &board.team_files);
                        (board, true)
                    }
                    Some(Err(e)) => bail!("{e}"),
                    None => (engine::light_snapshot(), false),
                },
                false => (engine::light_snapshot(), false),
            };
            self.board = Some(board);
            self.from_app = from_app;
            self.veiled = None;
        }
        Ok(self.board.as_ref().unwrap())
    }

    /// Cuts the sessions of a boundary group the session running this command isn't in to their
    /// metadata (boundary.rs `veil`), on this Mac's board and your other Macs', and notes which.
    /// Nothing changes for a person's own terminal, or with no boundary group in config.toml.
    fn veil(&mut self) -> Result<()> {
        // Settings that don't read stop the board too: which sessions to keep in can't be known.
        // Noted only once it's done, so a read after one that failed tries again, never gets
        // the board whole.
        let cfg = self.cfg()?.clone();
        if !cfg.routing.groups.iter().any(|g| g.boundary) {
            self.veiled = Some(HashSet::new());
            return Ok(());
        }
        let reader = self.reader()?;
        let mut keys = HashSet::new();
        for lanes in [self.board.as_mut().map(|b| &mut b.lanes), self.elsewhere.as_mut()].into_iter().flatten() {
            keys.extend(crate::boundary::veil_lanes(&cfg, reader.as_ref(), lanes));
        }
        self.veiled = Some(keys);
        Ok(())
    }

    /// Who's reading, for a boundary group: the account of the session running this command and
    /// whether its sign-in qualifies, as the server keeps it (`account_check_on`), else as this
    /// command finds it. None for a person's own terminal.
    pub(super) fn reader(&mut self) -> Result<Option<crate::boundary::Reader>> {
        let check = match self.app.is_some() {
            true => match server::ask_within(&check_request(None), CHECK_WAIT) {
                Some(Ok(v)) => serde_json::from_value::<AccountCheck>(v).ok(),
                _ => None,
            },
            false => None,
        };
        let check = match check {
            Some(c) => c,
            None => {
                let me = find_me(self)?;
                let cfg = self.cfg()?.clone();
                account_check_of(self.raw_board()?, &cfg, me, None, crate::signin::Ask::IfStale, now_ms()).map_err(|e| anyhow!(e))?
            }
        };
        // Found by the variable alone, which an agent can set itself, it reads as a session
        // outside any boundary: never one inside.
        let trusted = check.how.as_deref() != Some("environment");
        Ok(check.account.filter(|_| check.found).map(|account| crate::boundary::Reader { account, qualifies: trusted && check.sign_in.is_some_and(|s| s.qualifies()) }))
    }

    /// Whether the session running this command may not have what the session `key` says.
    pub(super) fn veils(&mut self, key: &str) -> Result<bool> {
        self.board()?;
        if crate::peerfeed::split_remote(key).is_some() {
            self.elsewhere();
        }
        Ok(self.veiled.as_ref().is_some_and(|v| v.contains(key)))
    }

    /// The board, for looking up a session or worktree by what the command was given: until the
    /// app's first scan after a start ends (`loading`), its board has none, and anything named
    /// would be taken for something Codealong doesn't know. That's waited for, at most
    /// LOADING_WAIT; a board still loading after that is returned as it is, for the lookup to say
    /// so (`no_session`).
    pub(super) fn loaded_board(&mut self) -> Result<&Board> {
        if self.board()?.loading && self.from_app {
            self.wait_loaded(LOADING_WAIT);
        }
        self.board()
    }

    /// Waits, at most `within`, for the app's first scan after a start to end (`loaded_within`),
    /// asking its status rather than its board, which is a megabyte or more; the board is read
    /// again after. An app that stops answering meanwhile is restarting again, and the files
    /// answer until it's back (`board`).
    fn wait_loaded(&mut self, within: std::time::Duration) {
        let status = |wait| match server::ask_within(&Request::Status, wait) {
            Some(Ok(v)) => serde_json::from_value::<server::Status>(v).ok(),
            _ => None,
        };
        if let Some(status) = loaded_within(within, status) {
            self.app = Some(status);
        }
        self.board = None;
    }

    /// The board's worktrees, for whether a todo's PR is in review, when the app answers: a
    /// light board reads no PRs, and isn't worth reading for none.
    pub(super) fn pr_lanes(&mut self) -> Vec<Lane> {
        match self.app.is_some() {
            true => self.board().map(|b| b.lanes.clone()).unwrap_or_default(),
            false => Vec::new(),
        }
    }

    /// Your other Macs' worktrees with their sessions, as the windows show them beside the board
    /// (server.rs `Elsewhere`): every key with `@<short>:` in front. None without the server, or
    /// from a server from before it said them.
    pub(super) fn elsewhere(&mut self) -> &[Lane] {
        if self.elsewhere.is_none() {
            let lanes = match self.app.is_some() {
                true => match server::ask(&Request::Elsewhere) {
                    Some(Ok(v)) => serde_json::from_value(v).unwrap_or_default(),
                    _ => Vec::new(),
                },
                false => Vec::new(),
            };
            self.elsewhere = Some(lanes);
            // Veiled with this Mac's board, once its reader is known; none at all when it can't be.
            if self.veiled.is_some() {
                self.veiled = None;
                if self.board().is_err() {
                    self.elsewhere = Some(Vec::new());
                }
            }
        }
        self.elsewhere.as_deref().unwrap_or_default()
    }

    /// The session `key` names, and the worktree it's listed under, on this Mac's board or, for a
    /// key of another Mac's, on that Mac's.
    pub(super) fn session_anywhere(&mut self, key: &str) -> Result<Option<(Session, Lane)>> {
        if crate::peerfeed::split_remote(key).is_none() {
            return Ok(all_sessions(self.board()?).into_iter().find(|(s, _)| s.key == key).map(|(s, l)| (s.clone(), l.clone())));
        }
        Ok(all_in(self.elsewhere()).into_iter().find(|(s, _)| s.key == key).map(|(s, l)| (s.clone(), l.clone())))
    }

    /// Both read again, for a look that goes on (`session wait`).
    pub(super) fn forget(&mut self) {
        self.board = None;
        self.elsewhere = None;
    }

    /// The board read again, for a decision that mustn't go by one read a while ago.
    pub(super) fn fresh_board(&mut self) -> Result<&Board> {
        self.board = None;
        self.board()
    }

    /// Session key → a line of what was said, for sessions whose prompts and replies have
    /// every word: the app's index, or one built from the files now.
    pub(super) fn search_text(&mut self, words: &[String]) -> HashMap<String, String> {
        let from_app = match self.app.is_some() {
            true => match server::ask(&Request::Search { words: words.to_vec() }) {
                Some(Ok(v)) => serde_json::from_value::<search::Found>(v).ok(),
                _ => None,
            },
            false => None,
        };
        let found = from_app.unwrap_or_else(|| {
            // Without settings there are no accounts' records to look through: only titles match.
            let cfg = self.cfg().cloned().unwrap_or_default();
            search::find_now(search::Scope { accounts: cfg.accounts, days: cfg.search.days }, words, 1000)
        });
        found.hits.into_iter().map(|h| (h.key, h.snippet)).collect()
    }

    pub(super) fn todos(&mut self) -> Result<TodoStore> {
        self.note();
        if self.app.is_some() {
            let b = self.board()?;
            // Until its first scan ends, the app's board has no todos yet; the file has them.
            if !b.loading {
                return Ok(TodoStore { todos: b.todos.clone(), labels: b.labels.clone() });
            }
        }
        Ok(todos::load()?.0)
    }

    /// Applies a change: through the engine, or to the file under the lock. Returns whether
    /// anything changed. The cached board is dropped, so the next read shows the change.
    pub(super) fn apply(&mut self, op: TodoOp) -> Result<bool> {
        self.note();
        let needs_lanes = matches!(op, TodoOp::Link { .. } | TodoOp::Primary { .. });
        let out = match self.app.is_some() {
            true => match server::ask(&Request::TodoApply { op }) {
                Some(Ok(_)) => Ok(true),
                Some(Err(e)) => Err(anyhow!("{e}")),
                None => bail!("The app stopped answering."),
            },
            false => {
                // Only a link looks a worktree or a session up, and the board takes a moment to
                // read.
                let lanes = match needs_lanes {
                    true => todos::LaneIndex::of(&self.board()?.lanes),
                    false => todos::LaneIndex::default(),
                };
                let now = now_ms();
                todos::change(|store| Ok((store.apply(op, &lanes, now)?, ()))).map(|(_, changed, _)| changed)
            }
        };
        self.board = None;
        out
    }

    pub(super) fn start(&mut self, spec: &StartSpec) -> Result<start::Started> {
        self.note();
        if self.app.is_some() {
            return match server::ask(&Request::Start { spec: spec.clone() }) {
                Some(Ok(v)) => serde_json::from_value(v).context("reading the app's reply"),
                Some(Err(e)) => Err(anyhow!("{e}")),
                None => bail!("The app stopped answering."),
            };
        }
        if spec.place == Place::Codealong {
            // The session would run in this command's process and end with it.
            bail!("Starting a session in Codealong needs its server, which couldn't be started here (`codealong server start` says why).");
        }
        let cfg = self.cfg()?.clone();
        let board = self.board()?;
        // No engine to hand the todos' start to: it's written to todos.json from here, under
        // its lock.
        let started = start::start_and_link(board, &cfg, spec, |link| {
            let lanes = todos::LaneIndex::of(&board.lanes);
            let now = now_ms();
            todos::change(|store| {
                let mut changed = false;
                for id in &link.ids {
                    changed |= store.started(id, link.lane.as_deref(), link.session.as_deref(), link.pending.clone(), &lanes, now);
                }
                Ok((changed, ()))
            })?;
            Ok(())
        })?;
        if !spec.todos.is_empty() {
            self.board = None;
        }
        Ok(started)
    }

    /// One of the app's requests about the session `key` (actions.rs), asked as the command
    /// (`actions::From::Command`, with `from` the session running it when that's been looked
    /// for): through the app, which does what its windows do, or from here over the board when
    /// it's closed or older than the command.
    pub(super) fn act(&mut self, name: &str, key: &str, params: impl Serialize, from: Option<String>) -> Result<Value> {
        // A repo's own teardown can take minutes: removing a worktree, and Mark as done on a
        // background job, which removes the worktree it made.
        let wait = if name == api::WORKTREE_REMOVE || name == api::SESSION_DONE { REMOVAL_WAIT } else { server::WAIT };
        self.act_within(name, key, params, from, wait)
    }

    /// `act`, waiting at most `wait` for the app's answer.
    pub(super) fn act_within(&mut self, name: &str, key: &str, params: impl Serialize, from: Option<String>, wait: std::time::Duration) -> Result<Value> {
        self.note();
        // What a session said isn't handed to one outside its boundary group (boundary.rs): the
        // command turns it down here, and says who asks, so the server and another Mac do too.
        let mut from = from;
        if crate::actions::READS_TEXT.contains(&name) {
            if self.veils(key)? {
                bail!("{}", crate::boundary::REFUSED);
            }
            if from.is_none() {
                from = find_me(self)?.0;
            }
        }
        let params = serde_json::to_value(params)?;
        if self.app.is_some() {
            let run = Request::Run { name: name.to_string(), key: key.to_string(), params: params.clone(), from: from.clone() };
            let asked = std::time::Instant::now();
            let mut answer = server::ask_within(&run, wait);
            // An app that restarted after the board was read answers before its first scan has
            // ended, having done nothing: once it has, it's asked again, in what's left of `wait`.
            if matches!(&answer, Some(Err(e)) if e == actions::STILL_READING) {
                self.wait_loaded(wait.saturating_sub(asked.elapsed()).min(LOADING_WAIT));
                // Zero would wait for as long as it takes (`server::ask_within`).
                let left = wait.saturating_sub(asked.elapsed());
                if !left.is_zero() {
                    answer = server::ask_within(&run, left);
                }
            }
            match answer {
                Some(Ok(v)) => return Ok(v),
                // An app from before `run` says it doesn't know the request.
                Some(Err(e)) if !e.starts_with(server::UNKNOWN_REQUEST) => bail!("{e}"),
                Some(Err(_)) => {}
                None => bail!("The app stopped answering."),
            }
        }
        let engine = engine::Handle::over(self.board()?.clone());
        actions::local(&engine, &actions::From::Command(from), name, key, params).map_err(|e| anyhow!(e))
    }

    /// Brings the app's main window forward on a session, worktree, PR or todo (`Reveal` in
    /// api.ts). The app shows it, then this opens the app by its bundle ID: macOS may not let an
    /// app come forward by itself when nobody clicked in it, and an open from here is the doing of
    /// the app this command runs in (the terminal, or the agent's app). An open by ID goes to the
    /// copy that's running, where one by path could start a newer build of it.
    pub(super) fn show(&mut self, reveal: Value) -> Result<()> {
        self.note();
        let id = match server::ask(&Request::Show { reveal }) {
            Some(Ok(v)) => v.as_str().map(str::to_string),
            Some(Err(e)) if e.starts_with(server::UNKNOWN_REQUEST) => bail!("The Codealong that's running is older than this command and can't show a session. Update it, or quit it and open it again."),
            Some(Err(e)) => bail!("{e}"),
            None => bail!("Codealong isn't running, so there's no window to show it in."),
        };
        if let Some(id) = id {
            let _ = crate::os::open::in_app(crate::os::open::App::Id(&id), None, std::time::Duration::from_secs(10));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::tests::boundary_cfg;
    use crate::fixtures::lane;

    /// The wait for the app to finish reading after a start keeps to its time, however slowly the
    /// app answers, and ends as soon as it has read or stops answering.
    #[test]
    fn a_wait_for_the_app_to_finish_reading_keeps_to_its_time() {
        use std::time::{Duration, Instant};
        let status = |loading| server::Status { version: "0.4.0".into(), pid: 1, generated_at: 0, loading, ..Default::default() };
        let mut looks = 0;
        let read = loaded_within(Duration::from_secs(10), |_| {
            looks += 1;
            Some(status(looks < 2))
        });
        assert!(read.is_some_and(|s| !s.loading) && looks == 2);
        let mut looks = 0;
        let gone = loaded_within(Duration::from_secs(10), |_| {
            looks += 1;
            None
        });
        assert!(gone.is_none() && looks == 1);
        // Slow to answer and never done: each ask gets no more than what's left.
        let (within, began) = (Duration::from_millis(1200), Instant::now());
        let read = loaded_within(within, |wait| {
            assert!(began.elapsed() + wait <= within + Duration::from_millis(50), "{wait:?}");
            std::thread::sleep(wait);
            Some(status(true))
        });
        assert!(read.is_none() && began.elapsed() < within + Duration::from_millis(400), "{:?}", began.elapsed());
        assert!(loaded_within(Duration::ZERO, |_| panic!("asked with no time")).is_none());
    }

    /// A session outside a boundary group reads the group's sessions' metadata, not their words;
    /// a person's own terminal reads everything.
    #[test]
    fn a_boundarys_sessions_are_veiled_from_a_session_outside_it() {
        let member = Session { account: "claude-work".into(), excerpt: Some("SELECT * FROM customers".into()), ..crate::fixtures::session("claude:claude-work:k1") };
        let outside = Session { excerpt: Some("Fixing the parser".into()), ..crate::fixtures::session("claude:claude:k2") };
        let board = Board { lanes: vec![lane("/work/atlas", vec![member, outside])], ..Board::default() };
        let client = |me: Option<&str>| Client { app: None, verbose: false, cfg: Some(boundary_cfg()), board: Some(board.clone()), from_app: false, started: false, said: false, me: Some((me.map(String::from), me.map(|_| "process"))), elsewhere: None, veiled: None };
        let mut c = client(Some("claude:claude:k2"));
        let b = c.board().unwrap();
        let excerpts: Vec<Option<&str>> = b.lanes[0].sessions.iter().map(|s| s.excerpt.as_deref()).collect();
        assert_eq!(excerpts, [None, Some("Fixing the parser")]);
        assert!(c.veils("claude:claude-work:k1").unwrap() && !c.veils("claude:claude:k2").unwrap());
        let mut person = client(None);
        assert_eq!(person.board().unwrap().lanes[0].sessions[0].excerpt.as_deref(), Some("SELECT * FROM customers"));
    }
}
