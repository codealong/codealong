//! `codealong sessions` (the list, `find`, `close-idle`) and `codealong session`: one session
//! shown, opened, read, sent to, waited on, tagged, labelled, snoozed, raised, marked done or seen.
//!
//! - A session is named through lookup.rs: `resolve_any_session` where the request goes to the
//!   Mac the session is on, `resolve_session` where it has to be on this one.
//! - What's done to a session is the app's own request (`Client::act`, actions.rs). A new action
//!   is a request there with its params, and here only its arguments and what it prints;
//!   `session_act` is for the ones that only say what happened (stop, rename, fork, deny).
//! - A send, a tag, suggestions, a raise, done and seen go as the session running the command
//!   (`find_me`): the app says whose they are, and holds an agent to what it may ask.
//! - `SessionRow` is a session as every list prints it; `pr show` and `whoami` print it too.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};

use crate::actions::{self, params};
use crate::api;
use crate::board;
use crate::config::AccountCfg;
use crate::insights;
use crate::model::{Lane, Session, Status};
use crate::search;
use crate::server::{self, Request};
use crate::todos::Todo;
use crate::util::{now_ms, one_line, tilde, truncate};

use super::*;

// ---- sessions ----

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(super) struct SessionRow {
    pub(super) key: String,
    id: String,
    pub(super) account: String,
    pub(super) harness: String,
    pub(super) status: String,
    live: bool,
    archived: bool,
    pub(super) title: String,
    excerpt: Option<String>,
    headline: String,
    cwd: String,
    pub(super) worktree: String,
    project: String,
    branch: Option<String>,
    pr: Option<u64>,
    started_at: Option<i64>,
    last_activity: i64,
    last_prompt_at: i64,
    resume_command: Option<String>,
    attach_command: Option<String>,
    /// Ids of todos linked to it.
    todos: Vec<String>,
    /// The names of the labels on it (`session label`), in the order they were put on.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    labels: Vec<String>,
    /// Replies suggested for its user to send (`session suggest`), while it has them.
    #[serde(skip_serializing_if = "Option::is_none")]
    suggestions: Option<crate::model::Suggestions>,
    /// Where it stands as read from its last reply and what it waits on: the decision model's read (`[decisions]
    /// sessions`) and an agent's tag (`session tag`, `by`), where there is one. `asksYou` and
    /// `finished` are 0 to 1, `risk` 0 to 3, `landing` and `background` by name.
    #[serde(skip_serializing_if = "Option::is_none")]
    insight: Option<crate::model::SessionInsight>,
    /// Its work is done and nothing is left for its user, and `why`: `sure` when something
    /// besides its last reply says so (a PR of its merged, a todo of its done, its status line),
    /// not when only the read of that reply does. Left out while it isn't.
    #[serde(skip_serializing_if = "Option::is_none")]
    finished: Option<crate::model::Finished>,
    /// What it helps, when it's a helper (`start --helper`, or an automatic review): `parent`,
    /// the session whose agent started it, or `pr`, the PR it reviews. Its turns are for that
    /// agent, not its user.
    #[serde(skip_serializing_if = "Option::is_none")]
    helper: Option<crate::model::Helper>,
    /// A run nobody is at (`claude -p`, `codex exec`): what ran it reads its replies.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    headless: bool,
    /// Where a search found the words in what was said, when it did: a line of the message.
    #[serde(skip_serializing_if = "Option::is_none")]
    said: Option<String>,
}

pub(super) fn session_row(s: &Session, lane: &Lane, todos: &[Todo]) -> SessionRow {
    SessionRow {
        key: s.key.clone(),
        id: s.id.clone(),
        account: s.account.clone(),
        harness: lower_debug(&s.harness),
        status: lower_debug(&s.status),
        live: s.live,
        archived: s.archived,
        title: s.title.clone(),
        excerpt: s.excerpt.clone(),
        headline: s.headline.clone(),
        cwd: s.cwd.clone(),
        worktree: lane.id.clone(),
        project: lane.project.clone(),
        branch: s.branch.clone().or_else(|| lane.branch.clone()),
        pr: lane.pr.as_ref().map(|p| p.number),
        started_at: s.started_at,
        last_activity: s.last_activity,
        last_prompt_at: s.last_prompt_at,
        resume_command: s.resume_command.clone(),
        attach_command: s.attach_command.clone(),
        todos: todos.iter().filter(|t| t.sessions.contains(&s.key)).map(|t| t.id.clone()).collect(),
        labels: s.labels.iter().map(|l| l.name.clone()).collect(),
        suggestions: s.suggestions.clone(),
        insight: s.insight.clone(),
        finished: s.finished.clone(),
        helper: s.helper.clone(),
        headless: s.headless,
        said: None,
    }
}

pub(super) fn sessions(c: &mut Client, f: &SessionFilter, query: &[String], json: bool) -> Result<()> {
    let since = parse_since(&f.since)?;
    let status = f.status.as_deref().map(parse_status).transpose()?;
    let worktree = f.worktree.as_deref().map(|w| resolve_worktree(c, w)).transpose()?;
    let words: Vec<String> = query.iter().flat_map(|q| q.split_whitespace()).map(str::to_lowercase).collect();
    let said = if words.is_empty() { HashMap::new() } else { c.search_text(&words) };
    let me = if words.is_empty() { None } else { find_me(c)?.0 };
    let wanted = Wanted { status, worktree, since, words, said, me, now: now_ms() };
    // The board first, so your other Macs' sessions come veiled as this Mac's do.
    c.board()?;
    let elsewhere = if f.devices { c.elsewhere().to_vec() } else { Vec::new() };
    // A search doesn't say what a session it may not read said, nor that it said the words.
    let mut wanted = wanted;
    let veiled = c.veiled.clone().unwrap_or_default();
    wanted.said.retain(|k, _| !veiled.contains(k));
    let board = c.board()?;
    let mut found: Vec<(&Session, &Lane)> = all_sessions(board).into_iter().chain(all_in(&elsewhere)).filter(|(s, lane)| wanted.lists(f, s, lane)).collect();
    wanted.sort(&mut found);
    found.truncate(f.limit);
    let rows: Vec<SessionRow> = found.into_iter().map(|(s, lane)| SessionRow { said: wanted.said.get(&s.key).cloned(), ..session_row(s, lane, &board.todos) }).collect();
    if json {
        return print_json(&rows);
    }
    // A finished one says so beside its status, as its row in Codealong does.
    let status = |r: &SessionRow| match &r.finished {
        Some(f) => format!("{}, {}", r.status, if f.sure { "finished" } else { "looks finished" }),
        None => r.status.clone(),
    };
    let table: Vec<Vec<String>> = rows
        .iter()
        .map(|r| vec![status(r), r.account.clone(), ago(wanted.now - r.last_activity), r.worktree_short(), truncate(&r.title, 50), truncate(r.said.as_deref().or(r.excerpt.as_deref()).unwrap_or(""), 60), r.key.clone()])
        .collect();
    print!("{}", table_text(&["STATUS", "ACCOUNT", "AGO", "WORKTREE", "TITLE", "EXCERPT", "KEY"], table));
    Ok(())
}

/// What `sessions` was asked for, read: a filter's status, worktree and age, and for a search its
/// words, the sessions whose prompts or replies have them (`said`, with where), and the session
/// running it (`me`), which a search leaves out: its own conversation has the words it searches
/// for, and the question is whether another session has them.
struct Wanted {
    status: Option<Status>,
    worktree: Option<String>,
    since: Option<i64>,
    words: Vec<String>,
    said: HashMap<String, String>,
    me: Option<String>,
    now: i64,
}

impl Wanted {
    /// Whether `sessions` lists `s`, from `lane`.
    fn lists(&self, f: &SessionFilter, s: &Session, lane: &Lane) -> bool {
        (f.archived || !s.archived)
            && (f.headless || !s.headless)
            && self.me.as_deref() != Some(s.key.as_str())
            && f.account.as_deref().map_or(true, |a| a == s.account)
            && self.status.map_or(true, |st| st == s.status)
            && (!f.waiting || (matches!(s.status, Status::Needs | Status::Error | Status::Unread) && !s.held_back()))
            && (!f.finished || s.finished.is_some())
            && f.project.as_deref().map_or(true, |p| p.eq_ignore_ascii_case(&lane.project))
            && self.worktree.as_deref().map_or(true, |w| w == lane.id)
            && (!f.live || s.live)
            && (f.labels.is_empty() || f.labels.iter().any(|n| s.labels.iter().any(|l| l.name.eq_ignore_ascii_case(n.trim()))))
            && self.since.map_or(true, |ms| self.now - s.last_activity <= ms)
            && (self.words.is_empty() || self.rank(s, lane) < Match::Said || self.said.contains_key(&s.key))
    }

    /// Puts what `sessions` lists in order: for a search, by how closely it names each session
    /// (`rank`); then those that need you, the longest wait first as on the bar; then the one
    /// you last prompted.
    fn sort(&self, found: &mut [(&Session, &Lane)]) {
        let waited = |s: &Session| board::waiting_since(s).map_or((1, 0), |t| (0, t));
        found.sort_by_cached_key(|(s, lane)| (self.rank(s, lane), waited(s), std::cmp::Reverse(s.last_prompt_at)));
    }

    /// Where a search's words are in `s`, from its row: someone finding a session by its name
    /// wants it before the sessions that only mention it. `Said` when they're only in what was
    /// said, or nowhere; without words every session is `Exact`, and the listing's order stands.
    fn rank(&self, s: &Session, lane: &Lane) -> Match {
        if self.words.is_empty() {
            return Match::Exact;
        }
        let names: Vec<String> = [Some(&s.title), s.name.as_ref()].into_iter().flatten().map(|n| n.to_lowercase()).collect();
        let asked = self.words.join(" ");
        if names.iter().any(|n| n.split_whitespace().collect::<Vec<_>>().join(" ") == asked) {
            return Match::Exact;
        }
        let has_all = |hay: &str| self.words.iter().all(|w| search::has_word(hay, w, self.words.len()));
        // Each name on its own: words split between a title and a name don't name the session.
        if names.iter().any(|n| has_all(n)) {
            return Match::InName;
        }
        let pr = lane.pr.as_ref().map(|p| format!("#{} {}", p.number, p.title)).unwrap_or_default();
        let row = [s.account.as_str(), s.excerpt.as_deref().unwrap_or(""), lane.callsign.as_str(), lane.repo.as_str(), lane.branch.as_deref().unwrap_or(""), &pr].join(" ").to_lowercase();
        match has_all(&format!("{} {row}", names.join(" "))) {
            true => Match::InRow,
            false => Match::Said,
        }
    }
}

/// Where `sessions find` found its words in a session, the closest first: its title or name is
/// them; its title or name has them all; its row has them (account, excerpt, worktree, repo,
/// branch, PR, with its title and name); only its prompts and replies do.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Match {
    Exact,
    InName,
    InRow,
    Said,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CloseIdleOut {
    closed: Vec<crate::idle::Outcome>,
    kept: Vec<crate::idle::Kept>,
}

pub(super) fn close_idle(c: &mut Client, after: &str, archive: bool, dry_run: bool, json: bool) -> Result<()> {
    use crate::idle::{self, Ends};
    let after_ms = idle::parse_after(after).ok_or_else(|| anyhow!("--after is like 90m, 24h, 3d or 1w, not {after}."))?;
    c.note();
    let via_app = c.app.is_some();
    // What's archived is the app's to keep.
    if archive && !via_app {
        bail!("--archive needs Codealong's server, which couldn't be started here (`codealong server start` says why).");
    }
    let plan: idle::Plan = match via_app {
        true => match server::ask(&Request::IdleSessions { after_ms, archive }) {
            Some(Ok(v)) => serde_json::from_value(v).context("reading the app's reply")?,
            Some(Err(e)) => bail!("{e}"),
            None => bail!("The app stopped answering."),
        },
        false => idle::plan_now(c.board()?, after_ms, false),
    };
    let now = now_ms();
    let nothing = format!("No session has sat idle for {after} with nothing asked of you.");
    if dry_run {
        if json {
            return print_json(&plan);
        }
        if plan.close.is_empty() {
            println!("{nothing}");
        } else {
            let does = |ends: Ends| match ends {
                Ends::Terminal => "close tab",
                Ends::Job => "stop job",
                Ends::Archive => "archive",
            };
            let rows = plan.close.iter().map(|i| vec![does(i.ends).to_string(), i.account.clone(), ago(now - i.last_activity), i.callsign.clone(), truncate(&i.title, 50), i.why.clone(), i.key.clone()]).collect();
            print!("{}", table_text(&["DOES", "ACCOUNT", "IDLE", "WORKTREE", "TITLE", "WHY", "KEY"], rows));
        }
        print_kept(&plan.kept);
        return Ok(());
    }
    let keys: Vec<String> = plan.close.iter().map(|i| i.key.clone()).collect();
    let closed = match (keys.is_empty(), via_app) {
        (true, _) => Vec::new(),
        (false, true) => match server::ask(&Request::CloseIdle { keys, after_ms, archive }) {
            Some(Ok(v)) => serde_json::from_value(v).context("reading the app's reply")?,
            Some(Err(e)) => bail!("{e}"),
            None => bail!("The app stopped answering."),
        },
        (false, false) => {
            let cfg = c.cfg()?.clone();
            idle::close(c.board()?, &cfg, &keys, after_ms, None)
        }
    };
    if json {
        return print_json(&CloseIdleOut { closed, kept: plan.kept });
    }
    if closed.is_empty() {
        println!("{nothing}");
    } else {
        let result = |o: &idle::Outcome| match (o.ends, o.closed, o.archived) {
            (Some(Ends::Archive), _, true) => "archived",
            (Some(Ends::Job), true, _) => "stopped",
            (_, true, _) => "closed",
            _ => "kept",
        };
        let rows = closed.iter().map(|o| vec![result(o).to_string(), truncate(&o.title, 50), o.message.clone(), o.key.clone()]).collect();
        print!("{}", table_text(&["RESULT", "TITLE", "MESSAGE", "KEY"], rows));
    }
    print_kept(&plan.kept);
    Ok(())
}

/// One line on the sessions that stay open, by reason.
fn print_kept(kept: &[crate::idle::Kept]) {
    if kept.is_empty() {
        return;
    }
    let mut why: Vec<(String, usize)> = Vec::new();
    for k in kept {
        // "active 3 hours ago" differs per session; count those together.
        let reason = if k.why.starts_with("active ") { "active recently".to_string() } else { k.why.clone() };
        match why.iter_mut().find(|(r, _)| *r == reason) {
            Some((_, n)) => *n += 1,
            None => why.push((reason, 1)),
        }
    }
    let parts: Vec<String> = why.iter().map(|(r, n)| format!("{n} {r}")).collect();
    println!("Stay open: {}.", parts.join(", "));
}

impl SessionRow {
    fn worktree_short(&self) -> String {
        tilde(Path::new(&self.worktree))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionDetail {
    #[serde(flatten)]
    row: SessionRow,
    detail: Option<String>,
    name: Option<String>,
    status_since: Option<i64>,
    stalled_since: Option<i64>,
    pid: Option<u32>,
    worktree_callsign: String,
    repo: String,
    pr_title: Option<String>,
    pr_state: Option<String>,
    /// The session's own files: the transcript for Claude Code, the rollout for Codex, the
    /// database for OpenCode and Antigravity.
    records: Vec<String>,
    linked_todos: Vec<TodoRow>,
    /// PRs it worked on (`owner/name#number`), as `pr show` lists their sessions.
    prs: Vec<String>,
    /// The session it goes on as, when it moved to another account: messages go there.
    moved_to: Option<String>,
}

pub(super) fn session_show(c: &mut Client, key: &str, json: bool) -> Result<()> {
    let cfg = c.cfg()?.clone();
    let key = resolve_any_session(c, key)?;
    let found = c.session_anywhere(&key)?;
    let board = c.board()?;
    let (s, lane) = found.as_ref().map(|(s, l)| (s, l)).ok_or_else(|| no_session(board, &key))?;
    let row = session_row(s, lane, &board.todos);
    // Another Mac's session: its records, and where it moved to, are that Mac's to say.
    let here = crate::peerfeed::split_remote(&key).is_none();
    let account = cfg.accounts.iter().find(|a| here && a.name == s.account);
    let out = SessionDetail {
        detail: s.detail.clone(),
        name: s.name.clone(),
        status_since: s.status_since,
        stalled_since: s.stalled_since,
        pid: s.pid,
        worktree_callsign: lane.callsign.clone(),
        repo: lane.repo.clone(),
        pr_title: lane.pr.as_ref().map(|p| p.title.clone()),
        pr_state: lane.pr.as_ref().map(|p| lower_debug(&p.state)),
        records: account.map(|a| records_for(a, s)).unwrap_or_default(),
        linked_todos: board.todos.iter().filter(|t| t.sessions.contains(&s.key)).map(|t| todo_row(t, &store_of(board), &board.lanes, false)).collect(),
        prs: board.prs.iter().filter(|p| p.sessions.iter().any(|x| x.key == s.key)).map(|p| p.key.clone()).collect(),
        moved_to: if here { crate::origins::current().moved_to(&s.key, |k| board.session(k).is_some()) } else { None },
        row,
    };
    if json {
        return print_json(&out);
    }
    let r = &out.row;
    println!("{}  {} on {}{}", r.key, r.status, r.account, if r.live { " (live)" } else { "" });
    println!("Title: {}", r.title);
    if let Some(copy) = &out.moved_to {
        println!("Moved to: {copy} (send there)");
    }
    if let Some(e) = &r.excerpt {
        println!("Now: {e}");
    }
    if let Some(g) = &r.suggestions {
        let what = if g.replies.is_empty() { "Picked for its questions" } else { "Suggested replies" };
        println!("{what}{}:", g.by_title.as_deref().map(|t| format!(" (by {t})")).unwrap_or_default());
        for (i, reply) in g.replies.iter().enumerate() {
            println!("  {}. {}", i + 1, one_line(reply, 120));
        }
        for (question, labels) in &g.picks {
            let why = g.why.get(question).map(|w| format!(" ({w})")).unwrap_or_default();
            println!("  {}: {}{why}", one_line(question, 80), labels.join(", "));
        }
    }
    if let Some(i) = &r.insight {
        println!("Read: {}", insight_words(i));
    }
    if let Some(f) = &r.finished {
        println!("{}: {}", if f.sure { "Finished" } else { "Looks finished" }, f.why.join(", "));
    }
    if !r.labels.is_empty() {
        println!("Labels: {}", r.labels.join(", "));
    }
    println!("Worktree: {} ({}{})", tilde(Path::new(&r.worktree)), out.worktree_callsign, r.branch.as_deref().map(|b| format!(", {b}")).unwrap_or_default());
    if let Some(n) = r.pr {
        println!("PR: #{n} {} ({})", out.pr_title.clone().unwrap_or_default(), out.pr_state.clone().unwrap_or_default());
    }
    if let Some(cmd) = &r.resume_command {
        println!("Resume: {cmd}");
    }
    for p in &out.records {
        println!("Record: {}", tilde(Path::new(p)));
    }
    for t in &out.linked_todos {
        println!("Todo: {} {} ({})", t.id, t.title, t.status);
    }
    for p in &out.prs {
        println!("Worked on: {p}");
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenOut {
    /// The session the app shows.
    shown: String,
    title: String,
    worktree: String,
}

pub(super) fn session_open(c: &mut Client, key: &str, json: bool) -> Result<()> {
    if c.app.is_none() {
        bail!("Codealong isn't running, so there's no window to show a session in.");
    }
    let key = resolve_any_session(c, key)?;
    let found = c.session_anywhere(&key)?;
    let board = c.board()?;
    let (s, lane) = found.as_ref().map(|(s, l)| (s, l)).ok_or_else(|| no_session(board, &key))?;
    let out = OpenOut { shown: s.key.clone(), title: session_row(s, lane, &board.todos).title, worktree: lane.path.clone() };
    let reveal = json!({ "kind": "session", "laneId": lane.id, "key": s.key });
    c.show(reveal)?;
    if json {
        return print_json(&out);
    }
    println!("Codealong shows {} ({}).", out.title, out.shown);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SendOut {
    /// The session it went to.
    to: String,
    /// The session it says it's from: the one running this command, or null outside one.
    from: Option<String>,
    #[serde(flatten)]
    sent: crate::send::Sent,
}

pub(super) fn session_send(c: &mut Client, a: SendArgs, json: bool) -> Result<()> {
    let to = resolve_any_session(c, &a.key)?;
    let from = find_me(c)?.0;
    if from.as_deref() == Some(to.as_str()) {
        bail!("{to} is the session running this command.");
    }
    let text = with_attachments(read_text(a.message, a.message_file)?.unwrap_or_default(), &a.attach)?;
    if text.trim().is_empty() {
        bail!("Give a message, --message-file or --attach.");
    }
    // Run by a copy answering in a side conversation, which is no session of the board's: its
    // secret, which the engine checks is that of a copy of `to` (side.rs `sent_by_copy`).
    let side = std::env::var(crate::side::SIDE_VAR).unwrap_or_default();
    let sent = serde_json::from_value(c.act(api::SESSION_SEND, &to, params::Send { text, side, ..Default::default() }, from.clone())?).context("reading what the send said")?;
    if json {
        return print_json(&SendOut { to, from, sent });
    }
    println!("{}", sent.message);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PendingOut {
    session: String,
    pending: Option<api::Pending>,
}

pub(super) fn session_pending(c: &mut Client, key: &str, json: bool) -> Result<()> {
    let key = resolve_any_session(c, key)?;
    let pending = serde_json::from_value(c.act(api::SESSION_PENDING, &key, params::Session::default(), None)?).context("reading what it waits on")?;
    let out = PendingOut { session: key, pending };
    if json {
        return print_json(&out);
    }
    let Some(p) = out.pending else {
        println!("{} waits on nothing Codealong can see.", out.session);
        return Ok(());
    };
    for q in &p.questions {
        let choices: Vec<&str> = q.options.iter().map(|o| o.label.as_str()).collect();
        let header = q.header.as_deref().map(|h| format!("{h}: ")).unwrap_or_default();
        let several = if q.multi_select { "any of: " } else { "" };
        println!("Asks: {header}{} ({several}{})", q.question, choices.join(", "));
    }
    if p.questions.is_empty() {
        println!("Waits to run: {} (id {})", p.label, p.id);
    } else {
        println!("Their id: {} (for `session suggest --pick … --pending`)", p.id);
    }
    if let Some(input) = p.input.as_deref().filter(|i| !i.trim().is_empty()) {
        println!("{input}");
    }
    if let Some(why) = &p.cannot {
        println!("Codealong can't answer it: {why}");
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActOut {
    session: String,
    /// What happened, in a sentence, when the action says.
    said: Option<String>,
    /// A fork's copy, when its harness says it at once.
    #[serde(skip_serializing_if = "Option::is_none")]
    copy: Option<String>,
}

/// Stops, renames, forks or denies, through the app's own action (actions.rs), and says what
/// happened.
pub(super) fn session_act(c: &mut Client, key: &str, name: &str, params: impl Serialize, json: bool) -> Result<()> {
    let key = resolve_any_session(c, key)?;
    orchestrator_guard(c, name)?;
    let got = c.act(name, &key, params, None)?;
    let (said, copy) = match got {
        Value::String(s) => (Some(s), None),
        // A fork says what it started (`start::Started`).
        Value::Object(o) => (o.get("message").and_then(Value::as_str).map(String::from), o.get("key").and_then(Value::as_str).map(String::from)),
        _ => (None, None),
    };
    let said = said.or_else(|| (name == api::SESSION_RENAME).then(|| "Renamed.".to_string()));
    let out = ActOut { session: key, said, copy };
    if json {
        return print_json(&out);
    }
    if let Some(said) = &out.said {
        println!("{said}");
    }
    if let Some(copy) = &out.copy {
        println!("The copy: {copy}");
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SnoozeOut {
    /// The session's key, or the PR's `owner/name#number`.
    pub(super) key: String,
    /// When it comes back (ms since the epoch), or null once woken.
    pub(super) until: Option<i64>,
    pub(super) said: String,
}

impl SnoozeOut {
    pub(super) fn print(self, json: bool) -> Result<()> {
        if json {
            return print_json(&self);
        }
        println!("{}", self.said);
        Ok(())
    }
}

/// How a snooze went, in a sentence: `Snoozed until Fri 9 AM.`, or `Woken.`
pub(super) fn snooze_said(until: Option<i64>) -> String {
    let now = crate::util::now_ms();
    until.map_or_else(|| "Woken.".to_string(), |t| format!("Snoozed until {}.", crate::snoozewhen::words_at(t, now, &crate::schedules::rule::Local)))
}

/// `session snooze`: through the app, which keeps snoozes (a board read here would drop it:
/// `engine::Handle::over`) and brings the session back when its time comes.
pub(super) fn session_snooze(c: &mut Client, key: &str, when: &SnoozeWhen, json: bool) -> Result<()> {
    let until = when.until()?;
    if c.app.is_none() {
        bail!("Snoozing needs Codealong's server, which couldn't be started here (`codealong server start` says why): it keeps the snooze, and brings the session back when it ends.");
    }
    let key = resolve_any_session(c, key)?;
    // Another device's session is snoozed there, by the key its Mac knows it by.
    let own = crate::peerfeed::split_remote(&key).map_or(key.as_str(), |(_, own)| own).to_string();
    c.act(api::SESSION_SNOOZE, &key, params::Snooze { keys: vec![own], until }, None)?;
    SnoozeOut { key, until, said: snooze_said(until) }.print(json)
}

/// `session raise`: asked as the session running the command, which is named with it, through
/// the app, which keeps the move (as it keeps a snooze) on the session's Mac.
pub(super) fn session_raise(c: &mut Client, a: RaiseArgs, json: bool) -> Result<()> {
    if c.app.is_none() {
        bail!("Moving a session to the top needs Codealong's server, which couldn't be started here (`codealong server start` says why): it keeps the move, and its windows go by it.");
    }
    let key = resolve_any_session(c, &a.key)?;
    let me = find_me(c)?.0;
    let by_title = me.as_deref().and_then(|k| all_sessions(c.board().ok()?).into_iter().find(|(s, _)| s.key == k).map(|(s, _)| s.title.clone())).filter(|t| !t.is_empty());
    // Another device's session is moved there, by the key its Mac knows it by.
    let own = crate::peerfeed::split_remote(&key).map_or(key.as_str(), |(_, own)| own).to_string();
    let params = params::Raise { keys: vec![own], raised: !a.lower, agent: true, by: me.clone(), by_title, note: a.note };
    c.act(api::SESSION_RAISE, &key, params, me)?;
    let said = if a.lower { "Moved back to its place." } else { "Moved to the top of what waits on your user, after any they moved there themselves, until they send it something or see its finished turn." };
    let out = ActOut { session: key, said: Some(said.to_string()), copy: None };
    if json {
        return print_json(&out);
    }
    println!("{said}");
    Ok(())
}

/// `session done`: asked as the session running the command, which the app holds to what an
/// agent may mark done (agentdone.rs). The app keeps watch for the turn to end, so it must be up.
pub(super) fn session_done(c: &mut Client, key: &str, archive: bool, json: bool) -> Result<()> {
    if c.app.is_none() {
        bail!("Marking a session done needs Codealong's server, which couldn't be started here (`codealong server start` says why): it ends the session, and waits for this one's turn to end.");
    }
    let key = resolve_session(c, key)?;
    let me = find_me(c)?.0;
    let got = c.act(api::SESSION_DONE, &key, params::Done { archive, ..Default::default() }, me)?;
    let said = got.get("message").and_then(Value::as_str).map(String::from);
    let out = ActOut { session: key, said, copy: None };
    if json {
        return print_json(&out);
    }
    if let Some(said) = &out.said {
        println!("{said}");
    }
    Ok(())
}

/// `session seen`: asked as the session running the command, which may mark only its own turn
/// (actions.rs `agent_seen`), through the app, which keeps the mark as it keeps a snooze.
pub(super) fn session_seen(c: &mut Client, key: &str, note: Option<String>, json: bool) -> Result<()> {
    if c.app.is_none() {
        bail!("Marking a turn seen needs Codealong's server, which couldn't be started here (`codealong server start` says why): it keeps the mark, and its windows go by it.");
    }
    let key = resolve_session(c, key)?;
    let me = find_me(c)?.0;
    c.act(actions::AGENT_SEEN, &key, params::AgentSeen { note }, me)?;
    let said = "Marked seen: once this turn ends it's idle rather than Your turn, unless its reply asks your user something or a prompt comes first.";
    let out = ActOut { session: key, said: Some(said.to_string()), copy: None };
    if json {
        return print_json(&out);
    }
    println!("{said}");
    Ok(())
}

/// How much of a tool call's input or output `session transcript` shows without --full: enough
/// to tell what it did and how it went, not a file's whole text.
const TRANSCRIPT_CUT: usize = 600;

/// `session transcript`: a page of the conversation, as a window's panel reads it (the
/// session-page request), on whichever device the session is on.
pub(super) fn session_transcript(c: &mut Client, a: &TranscriptArgs, json: bool) -> Result<()> {
    let key = resolve_any_session(c, &a.key)?;
    // Taken before the page is read, so a prompt sent meanwhile makes it older than what was read,
    // never newer: replies suggested for it are then turned down, not put on a turn they didn't
    // see.
    let prompt_at = c.session_anywhere(&key)?.map(|(s, _)| s.last_prompt_at);
    let got = c.act(api::SESSION_PAGE, &key, params::Page { before: a.before, ..Default::default() }, None)?;
    let mut page: api::Page = serde_json::from_value(got).context("The conversation didn't read")?;
    if !a.full {
        for item in &mut page.items {
            for text in [&mut item.input, &mut item.output].into_iter().flatten() {
                if text.chars().count() > TRANSCRIPT_CUT {
                    *text = format!("{}… (cut; --full for all of it)", text.chars().take(TRANSCRIPT_CUT).collect::<String>());
                }
            }
        }
    }
    if json {
        return print_json(&json!({ "session": key, "lastPromptAt": prompt_at, "items": page.items, "before": page.before }));
    }
    let now = now_ms();
    for item in &page.items {
        let when = match ago(now - item.at).as_str() {
            "now" => "just now".to_string(),
            a => format!("{a} ago"),
        };
        match item.kind.as_str() {
            "you" => println!("\n── You · {when}{}\n{}", if item.withdrawn { " (taken back)" } else { "" }, item.text.trim_end()),
            "agent" => println!("\n── Agent · {when}\n{}", item.text.trim_end()),
            "peer" => println!("\n── From {} · {when}\n{}", item.from.as_deref().map_or_else(|| "another session".to_string(), |f| format!("“{f}”")), item.text.trim_end()),
            "tool" => println!("  {} {}", if item.failed { "✗" } else { "⏺" }, one_line(&item.text, 160)),
            "result" => {}
            _ => println!("  · {}", one_line(&item.text, 160)),
        }
    }
    if let Some(before) = page.before {
        println!("\nEarlier: codealong session transcript {key} --before {before}");
    }
    if let Some(at) = prompt_at {
        println!("Suggest replies to this with --prompt-at {at}");
    }
    Ok(())
}

/// How often `session wait` looks at the session again.
const WAIT_EVERY: std::time::Duration = std::time::Duration::from_secs(3);
/// How long `session wait` waits for a session the board doesn't have yet to show up: one just
/// started can take a moment.
const WAIT_TO_SHOW_MS: i64 = 2 * 60_000;
/// How long `session wait` waits on a session Codealong runs whose turn Codealong quitting cut off,
/// for Codealong to tell it to carry on as it comes back (reconnect.rs; autoreview.rs for a
/// review) and the turn to go on: seconds, its agent being picked up again in a new process.
const WAIT_CARRY_ON_MS: i64 = 3 * 60_000;

/// Where a session `session wait` waits on stands.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Waited {
    /// Its turn is running.
    Working,
    /// It waits on its user: a call to allow, a question, an error.
    OnUser,
    /// Not in a turn: its turn ended, or it hasn't started one yet.
    Still,
}

fn waited(s: &Session) -> Waited {
    match s.status {
        Status::Working => Waited::Working,
        _ if s.waits_on_its_user() => Waited::OnUser,
        _ => Waited::Still,
    }
}

/// A turn's last reply: its last run of replies, once that run came after `since`. None while the
/// turn hasn't written one, or ended on something else (a call, a prompt).
fn reply_since(items: &[api::Item], since: i64) -> Option<String> {
    let end = items.iter().rposition(|i| i.kind != "note")?;
    let start = items[..=end].iter().rposition(|i| i.kind != "agent").map_or(0, |i| i + 1);
    let run = &items[start..=end];
    if run.is_empty() || run.iter().any(|i| i.kind != "agent" || i.at < since) {
        return None;
    }
    let text = run.iter().map(|i| i.text.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n");
    (!text.is_empty()).then_some(text)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WaitOut {
    session: String,
    status: Status,
    /// Its turn is over.
    ended: bool,
    /// It waits on its user, who answers it in Codealong.
    waits_on_user: bool,
    reply: Option<String>,
    /// What it's doing or waits on, as the board says.
    excerpt: Option<String>,
    last_prompt_at: i64,
}

/// How long a session `session wait` never saw working has to sit still, with no new prompt, for
/// its turn to count as over without a reply after what's waited on: one that ended on a call, or
/// was stopped, never writes one.
const WAIT_STILL_MS: i64 = 60_000;

/// What `session wait` makes of a look at the session.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Seen {
    /// Its turn runs, or hasn't started.
    Going,
    Ended,
    OnUser,
}

/// What `session wait` saw of the session over its looks.
#[derive(Debug, Default)]
struct Watch {
    /// `--until-done`: a call to allow or a question is part of the turn, waited through.
    through_user: bool,
    /// `[sessions] continue_after_restart`: a session Codealong runs, cut off by Codealong
    /// quitting, is told to carry on as it comes back.
    carries_on: bool,
    was_working: bool,
    /// Its last prompt when a look first found it still, and when.
    still: Option<(i64, i64)>,
    /// When a look first found it cut off, to be carried on.
    cut: Option<i64>,
}

impl Watch {
    /// One look at `s` at `now`. A turn ends once it's been seen working and has stopped; when it
    /// was never seen working (it was quick, or this ran late), once it holds still across two
    /// looks with no new prompt between and `replied` (asked only then) says its record has a
    /// reply after what's waited on, or once it has held still a minute. The first look can come
    /// before a message just sent reaches the board, and the reply it finds then is the turn
    /// before's. A turn Codealong quitting cut off goes on once Codealong tells it to: it's waited
    /// through for a few minutes, after which it waits on its user, shown as an error or, seen
    /// already, idle: the turn didn't end.
    fn look(&mut self, s: &Session, now: i64, replied: impl FnOnce() -> bool) -> Seen {
        if self.carries_on && crate::reconnect::cut_off_here(s) {
            if now - *self.cut.get_or_insert(now) >= WAIT_CARRY_ON_MS {
                return Seen::OnUser;
            }
            self.was_working = true;
            self.still = None;
            return Seen::Going;
        }
        self.cut = None;
        match waited(s) {
            Waited::Working => {
                self.was_working = true;
                self.still = None;
                Seen::Going
            }
            // An error ends the turn, so it comes back whatever was asked. A sign-in turned down
            // doesn't: once there's one, Codealong tells it to carry on (reconnect.rs).
            Waited::OnUser if self.through_user && (s.status == Status::Needs || s.signed_out.is_some()) => {
                self.was_working = true;
                self.still = None;
                Seen::Going
            }
            Waited::OnUser => Seen::OnUser,
            Waited::Still if self.was_working => Seen::Ended,
            Waited::Still => match self.still {
                Some((prompt, at)) if prompt == s.last_prompt_at => {
                    if now - at >= WAIT_STILL_MS || replied() {
                        Seen::Ended
                    } else {
                        Seen::Going
                    }
                }
                _ => {
                    self.still = Some((s.last_prompt_at, now));
                    Seen::Going
                }
            },
        }
    }
}

/// `session wait`'s --since, as given or as --since-file holds it.
fn wait_since(a: &SessionWaitArgs) -> Result<Option<i64>> {
    let Some(t) = read_text(None, a.since_file.clone())? else { return Ok(a.since) };
    let t = t.trim();
    t.parse().map(Some).map_err(|_| anyhow!("--since-file should hold a time in epoch ms (`date +%s000`), not {t:?}."))
}

/// `session wait`: looks at the session every few seconds until its turn ends or it waits on its
/// user (`Watch::look`), and prints its reply.
pub(super) fn session_wait(c: &mut Client, a: &SessionWaitArgs, json: bool) -> Result<()> {
    // A helper just started may not be on the board yet: its whole key, as `start` printed it, is
    // waited for until it shows up.
    let key = match resolve_any_session(c, &a.key) {
        Ok(k) => k,
        Err(_) if a.key.matches(':').count() >= 2 => a.key.clone(),
        Err(e) => return Err(e),
    };
    let timeout = crate::idle::parse_after(&a.timeout).ok_or_else(|| anyhow!("--timeout is like 30m or 2h, not {}.", a.timeout))?;
    let started = now_ms();
    let mut since = wait_since(a)?;
    let mut seen = false;
    let carries_on = c.cfg().is_ok_and(|cfg| cfg.sessions.continue_after_restart);
    let mut watch = Watch { through_user: a.until_done, carries_on, ..Watch::default() };
    let page = |c: &mut Client| -> Result<Vec<api::Item>> {
        // Its reply isn't this session's to read when it's in a boundary group this one isn't in.
        if c.veils(&key)? {
            return Ok(Vec::new());
        }
        let got = c.act(api::SESSION_PAGE, &key, params::Page::default(), None)?;
        Ok(serde_json::from_value::<api::Page>(got).context("The conversation didn't read")?.items)
    };
    loop {
        let found = c.session_anywhere(&key)?.map(|(s, _)| s);
        let loading = c.board()?.loading;
        // The app answered as the wait began and doesn't now, or answers with the empty board it
        // has before its first scan: it's restarting (another session's relaunch, an update). A
        // board read from the files has nothing of a session it runs but its record, which ends a
        // cut-off turn as stopped, so nothing is judged until it's back.
        let restarting = c.app.is_some() && (!c.from_app || loading);
        match found {
            _ if restarting => {}
            None if seen => bail!("{key} is gone from the board."),
            None if now_ms() - started > WAIT_TO_SHOW_MS => bail!("There's no session {key}."),
            None => {}
            Some(s) => {
                seen = true;
                let since = *since.get_or_insert(s.last_prompt_at);
                // The record is read at most once a look, and kept for the reply.
                let mut items: Option<Vec<api::Item>> = None;
                let seen_now = watch.look(&s, now_ms(), || match page(c) {
                    Ok(got) => reply_since(&items.insert(got)[..], since).is_some(),
                    Err(_) => false,
                });
                let out = |ended: bool, reply: Option<String>| WaitOut { session: key.clone(), status: s.status, ended, waits_on_user: seen_now == Seen::OnUser, reply, excerpt: s.excerpt.clone(), last_prompt_at: s.last_prompt_at };
                match seen_now {
                    Seen::Going => {}
                    Seen::OnUser => return print_wait(&s, out(false, None), json),
                    Seen::Ended => {
                        let items = match items {
                            Some(i) => i,
                            None => page(c)?,
                        };
                        let reply = reply_since(&items, since).or_else(|| reply_since(&items, 0));
                        return print_wait(&s, out(true, reply), json);
                    }
                }
            }
        }
        if now_ms() - started > timeout {
            if restarting {
                bail!("Codealong stopped answering, so whether {key} is done can't be said: once it's open again, `codealong session wait {key}` waits on.");
            }
            bail!("{key} is still at it after {}: `codealong session wait {key}` waits on.", a.timeout);
        }
        std::thread::sleep(WAIT_EVERY);
        c.forget();
    }
}

fn print_wait(s: &Session, out: WaitOut, json: bool) -> Result<()> {
    if json {
        return print_json(&out);
    }
    if out.waits_on_user {
        let what = s.excerpt.as_deref().map(|e| format!(": {e}")).unwrap_or_default();
        println!("“{}” waits on your user{what}. They answer it in Codealong; wait again after.", s.title);
        return Ok(());
    }
    match &out.reply {
        Some(r) => println!("{}", r.trim_end()),
        None => println!("“{}” ended its turn without a reply.", s.title),
    }
    Ok(())
}

/// A session's read in a line: "pushed · finished 0.9 · asks you 0.1 (by Orchestrator: PR #12 is up)".
fn insight_words(i: &crate::model::SessionInsight) -> String {
    let mut parts = Vec::new();
    if let Some(l) = &i.landing {
        parts.push(l.clone());
    }
    if let Some(p) = i.finished {
        parts.push(format!("finished {p:.2}"));
    }
    if let Some(p) = i.asks_you {
        parts.push(format!("asks you {p:.2}"));
    }
    if let Some(e) = &i.ending {
        parts.push(format!("ending {e}"));
    }
    if let Some(r) = i.risk {
        parts.push(format!("risk {r:.1}"));
    }
    if let Some(p) = i.waits_for_input {
        parts.push(format!("waits for input {p:.2}"));
    }
    if let Some(b) = &i.background {
        parts.push(format!("background {b}"));
    }
    if let Some(t) = i.waits_until {
        // As a snooze's time is said.
        parts.push(format!("waits until {}", crate::snoozewhen::words_at(t, crate::util::now_ms(), &crate::schedules::rule::Local)));
    }
    if let Some(c) = i.confidence {
        parts.push(format!("confidence {c:.2}"));
    }
    let by = match (&i.by_title, &i.by, &i.note) {
        (None, None, _) => String::new(),
        (title, key, note) => format!(" (by {}{})", title.clone().or_else(|| key.clone()).unwrap_or_default(), note.as_ref().map(|n| format!(": {n}")).unwrap_or_default()),
    };
    format!("{}{by}", parts.join(" · "))
}

/// What `session tag`'s arguments say of a session whose last reply was written at `replied`, at
/// `now` on `zone`'s clock: nothing for --clear, which takes its tag away. An error says what's
/// missing or doesn't read.
pub(super) fn tag_said(a: TagArgs, replied: i64, zone: &dyn crate::schedules::rule::Zone) -> Result<params::Tag> {
    if a.clear {
        return Ok(params::Tag::default());
    }
    let read = a.asks_you.is_some() || a.ending.is_some() || a.landing.is_some() || a.finished.is_some() || a.risk.is_some() || a.background.is_some();
    if !read && a.waits_until.is_none() {
        bail!("Say something about it (--asks-you, --ending, --landing, --finished, --risk, --background or --waits-until), or --clear to take its tag away.");
    }
    // When its work goes on is part of the read of how its reply ends (insights.rs `merged`), so
    // it's said with that, never under a landing or a risk alone. That also keeps a Mac from
    // before 2026-11-11, which doesn't know it, from taking a tag that says nothing else for one
    // taking its tag away.
    if a.waits_until.is_some() && a.ending.is_none() && a.asks_you.is_none() {
        bail!("Say how its reply ends too (--ending): when its work goes on is part of that read.");
    }
    // Turned down here as well as on the session's Mac (tags.rs `read`), which may be older.
    if a.waits_until.is_some() && (a.asks_you == Some(true) || a.ending.as_deref().is_some_and(insights::asking)) {
        bail!("A reply that asks its user something waits on them now, not until a later time: leave out --waits-until, or say it ends some other way.");
    }
    // Read here so that what doesn't read is said at once, counted from when its reply was
    // written as far as this board says, for a Mac from before 2026-11-18. Whether it has passed
    // isn't this clock's to say: the session's Mac reads the words again (`waits_words`, tags.rs
    // `waits_until`), knowing when that was, on its own clock, which can be another zone's.
    let waits_until = a.waits_until.as_deref().map(|when| crate::snoozewhen::when(when, replied, zone).map_err(|e| anyhow!(e))).transpose()?;
    let unit = |b: Option<bool>| b.map(|b| if b { 1.0 } else { 0.0 });
    // An ending says whether the reply asks, and that goes with it: a Mac from before endings
    // reads only that, and would take a tag that says nothing else for one taking its tag away.
    let asks_you = unit(a.asks_you).or_else(|| a.ending.as_deref().map(|e| if insights::asking(e) { 1.0 } else { 0.0 }));
    Ok(params::Tag { asks_you, ending: a.ending, landing: a.landing, finished: unit(a.finished), risk: a.risk.map(f64::from), background: a.background, waits_until, waits_words: a.waits_until, note: a.note, prompt_at: a.prompt_at, ..Default::default() })
}

/// `session tag`: asked as the session running the command, which is named with it, on
/// whichever device the session is on. The app's windows show it at once; without the app, it's
/// kept for it.
pub(super) fn session_tag(c: &mut Client, a: TagArgs, json: bool) -> Result<()> {
    let key = resolve_any_session(c, &a.key)?;
    let me = find_me(c)?.0;
    let by_title = me.as_deref().and_then(|k| all_sessions(c.board().ok()?).into_iter().find(|(s, _)| s.key == k).map(|(s, _)| s.title.clone())).filter(|t| !t.is_empty());
    // When its last reply was written, as far as the board says (`tag_said`): here or on the
    // device it runs on.
    let now = crate::util::now_ms();
    let elsewhere = c.elsewhere().to_vec();
    let replied = all_sessions(c.board()?).into_iter().chain(all_in(&elsewhere)).find(|(s, _)| s.key == key).map_or(now, |(s, _)| crate::insights::replied_at(s));
    let params = params::Tag { by: me.clone(), by_title, ..tag_said(a, replied, &crate::schedules::rule::Local)? };
    let got = c.act(api::SESSION_TAG, &key, params, me)?;
    let out = ActOut { session: key, said: got.as_str().map(String::from), copy: None };
    if json {
        return print_json(&out);
    }
    if let Some(said) = &out.said {
        println!("{said}");
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LabelOut {
    session: String,
    /// The names of the labels it has now, in the order they were put on; null when its Mac
    /// didn't say (`labels_said`).
    labels: Option<Vec<String>>,
}

/// The labels the session `own` wears now, as its Mac answered the label request (actions.rs
/// `label`): another Mac's reaches this one's board a moment later. None when it didn't say: a
/// Codealong from before it did, or an engine too busy to show the change in time.
pub(super) fn labels_said(got: &Value, own: &str) -> Option<Vec<String>> {
    serde_json::from_value(got.get("labels")?.get(own)?.clone()).ok()
}

/// `session label`: asked of whichever device the session is on, whose labels they are, through
/// the server, which keeps them.
pub(super) fn session_label(c: &mut Client, a: LabelArgs, json: bool) -> Result<()> {
    if c.app.is_none() {
        bail!("Labelling a session needs Codealong's server, which couldn't be started here (`codealong server start` says why): it keeps the labels.");
    }
    let key = resolve_any_session(c, &a.key)?;
    // Another device's session is labelled there, by the key its Mac knows it by.
    let own = crate::peerfeed::split_remote(&key).map_or(key.as_str(), |(_, own)| own).to_string();
    let me = find_me(c)?.0;
    let got = c.act(api::SESSION_LABEL, &key, params::Label { keys: vec![own.clone()], add: a.add, remove: a.remove }, me)?;
    let labels = labels_said(&got, &own);
    let out = LabelOut { session: key, labels };
    if json {
        return print_json(&out);
    }
    match out.labels.as_deref() {
        Some([]) => println!("It has no labels."),
        Some(names) => println!("Labels: {}", names.join(", ")),
        None => println!("Labelled. Its Codealong didn't say which labels it has now: `codealong session show {}` does once it's there.", out.session),
    }
    Ok(())
}

/// `session suggest`: asked as the session running the command, which is named with them, on
/// whichever device the session is on. The app's windows show them at once; without the app,
/// they're kept for it.
pub(super) fn session_suggest(c: &mut Client, a: SuggestArgs, json: bool) -> Result<()> {
    let key = resolve_any_session(c, &a.key)?;
    let me = find_me(c)?.0;
    let by_title = me.as_deref().and_then(|k| all_sessions(c.board().ok()?).into_iter().find(|(s, _)| s.key == k).map(|(s, _)| s.title.clone())).filter(|t| !t.is_empty());
    let (replies, picks, mut why) = if a.clear { (Vec::new(), Vec::new(), Vec::new()) } else { (a.replies, a.picks, a.why) };
    if why.iter().all(|w| w.trim().is_empty()) {
        why.clear();
    }
    if !a.clear && replies.iter().chain(&picks).all(|r| r.trim().is_empty()) {
        bail!("Give at least one reply to suggest, a choice to --pick, or --clear to take them away.");
    }
    let params = params::Suggest { replies, picks, why, pending: a.pending, note: a.note, by: me.clone(), by_title, prompt_at: a.prompt_at, ..Default::default() };
    let got = c.act(api::SESSION_SUGGEST, &key, params, me)?;
    let out = ActOut { session: key, said: got.as_str().map(String::from), copy: None };
    if json {
        return print_json(&out);
    }
    if let Some(said) = &out.said {
        println!("{said}");
    }
    Ok(())
}

/// Where a session's own harness keeps it (`harness::Adapter::records`).
fn records_for(account: &AccountCfg, s: &Session) -> Vec<String> {
    crate::harness::adapter(s.harness).records(account, s).into_iter().map(|p| p.to_string_lossy().into_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::tests::session;
    use crate::fixtures::lane;
    use crate::model::Board;

    /// In an isolated worktree, Claude Code's guard turns down a `codealong` command that names git
    /// or reads a value in with `$(…)`: from 2026-10-10 to 2026-10-22, 219 helper starts in 137
    /// sessions, the review prompt naming git, and 8 waits whose --since was `$(cat …)`. A prompt
    /// and a time read from files leave it nothing to turn down.
    #[test]
    fn a_helper_takes_its_prompt_and_its_wait_takes_its_time_from_a_file() {
        let Cmd::Start(s) = Cli::try_parse_from(["codealong", "start", "--helper", "--account", "codex", "--prompt-file", "-"]).unwrap().cmd else { panic!() };
        assert_eq!((s.prompt, s.prompt_file.as_deref()), (None, Some("-")));
        for other in [&["--prompt", "Review it"][..], &["--todo", "t-a"], &["--take"]] {
            let line: Vec<&str> = ["codealong", "start", "--prompt-file", "review.txt"].into_iter().chain(other.iter().copied()).collect();
            assert!(Cli::try_parse_from(line).is_err(), "{other:?}");
        }
        let dir = std::env::temp_dir().join(format!("codealong-since-file-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("sent");
        let wait = |args: &[&str]| -> SessionWaitArgs {
            let line: Vec<&str> = ["codealong", "session", "wait", "abc"].into_iter().chain(args.iter().copied()).collect();
            let Cmd::Session { cmd: SessionCmd::Wait(w) } = Cli::try_parse_from(line).unwrap().cmd else { panic!() };
            w
        };
        let path = file.to_str().unwrap();
        std::fs::write(&file, "1793145600000\n").unwrap();
        assert_eq!(wait_since(&wait(&["--since-file", path])).unwrap(), Some(1793145600000));
        assert_eq!(wait_since(&wait(&["--since", "5"])).unwrap(), Some(5));
        assert_eq!(wait_since(&wait(&[])).unwrap(), None);
        std::fs::write(&file, "1m\n").unwrap();
        assert!(wait_since(&wait(&["--since-file", path])).unwrap_err().to_string().contains("epoch ms"));
        assert!(Cli::try_parse_from(["codealong", "session", "wait", "abc", "--since", "5", "--since-file", path]).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_wait_ends_on_the_reply_after_the_prompt_it_waits_on() {
        let item = |kind: &str, at: i64, text: &str| -> api::Item { serde_json::from_value(json!({ "at": at, "kind": kind, "text": text, "cursor": 0, "n": 0 })).unwrap() };
        let items = [item("you", 10, "Review it"), item("agent", 20, "Two problems."), item("agent", 21, "Both in src/a.rs."), item("note", 22, "compacted")];
        assert_eq!(reply_since(&items, 10).as_deref(), Some("Two problems.\n\nBoth in src/a.rs."));
        // A reply from before the message it waits on is the turn before's.
        assert_eq!(reply_since(&items, 21), None);
        // A turn that ended on a call has no reply to give.
        assert_eq!(reply_since(&[item("you", 10, "Go"), item("tool", 20, "Bash · `ls`")], 0), None);
        let s = |status| Session { status, ..Session::new(crate::model::Harness::Claude, "claude", "rev") };
        assert_eq!([Status::Working, Status::Needs, Status::Error, Status::Unread, Status::Idle, Status::Stopped].map(|st| waited(&s(st))), [Waited::Working, Waited::OnUser, Waited::OnUser, Waited::Still, Waited::Still, Waited::Still]);
    }

    #[test]
    fn a_wait_ends_once_the_turn_it_saw_stops_or_a_reply_holds_still() {
        let s = |status, prompt| Session { status, last_prompt_at: prompt, ..Session::new(crate::model::Harness::Claude, "claude", "rev") };
        let never = || -> bool { panic!("the record isn't read while the board says enough") };
        // Seen working, then stopped: over, without reading the record to decide.
        let mut w = Watch::default();
        assert_eq!(w.look(&s(Status::Working, 1), 0, never), Seen::Going);
        assert_eq!(w.look(&s(Status::Unread, 1), 3_000, never), Seen::Ended);
        // Waiting on its user comes back at once; with --until-done, a call to allow is waited
        // through to the turn's end, and only an error comes back.
        assert_eq!(Watch::default().look(&s(Status::Needs, 1), 0, never), Seen::OnUser);
        let mut w = Watch { through_user: true, ..Watch::default() };
        assert_eq!(w.look(&s(Status::Needs, 1), 0, never), Seen::Going);
        assert_eq!(w.look(&s(Status::Idle, 1), 3_000, never), Seen::Ended);
        assert_eq!(Watch { through_user: true, ..Watch::default() }.look(&s(Status::Error, 1), 0, never), Seen::OnUser);
        // A turn its account's sign-in stopped didn't end: it waits on its user, and with
        // --until-done it's waited through until Codealong carries it on and it ends.
        let out = |status| Session { signed_out: Some(crate::model::SignedOut { at: 1, said: "OAuth token revoked".into(), continues: true }), ..s(status, 1) };
        assert_eq!(waited(&out(Status::Idle)), Waited::OnUser);
        assert_eq!(Watch::default().look(&out(Status::Unread), 0, never), Seen::OnUser);
        let mut w = Watch { through_user: true, ..Watch::default() };
        assert_eq!(w.look(&out(Status::Unread), 0, never), Seen::Going);
        assert_eq!(w.look(&s(Status::Unread, 2), 3_000, never), Seen::Ended);
        // Never seen working: a reply after the prompt counts on the second still look, not the first.
        let mut w = Watch::default();
        assert_eq!(w.look(&s(Status::Idle, 1), 0, never), Seen::Going);
        assert_eq!(w.look(&s(Status::Idle, 1), 3_000, || true), Seen::Ended);
        // A message sent meanwhile is a new prompt: it starts again from there.
        let mut w = Watch::default();
        w.look(&s(Status::Idle, 1), 0, never);
        assert_eq!(w.look(&s(Status::Idle, 2), 3_000, never), Seen::Going);
        assert_eq!(w.look(&s(Status::Idle, 2), 6_000, || false), Seen::Going);
        // No reply after it (a turn that ended on a call): over once it has held still a minute.
        assert_eq!(w.look(&s(Status::Idle, 2), 3_000 + WAIT_STILL_MS, || false), Seen::Ended);
    }

    #[test]
    fn a_wait_goes_on_through_a_cut_codealong_carries_on() {
        let s = |status| Session { status, last_prompt_at: 1, ..Session::new(crate::model::Harness::Codex, "codex", "rev") };
        let cut = |status| Session { hosted: true, cut_off: Some(1_000), ..s(status) };
        let never = || -> bool { panic!("the record isn't read while the board says enough") };
        // Codealong quit under its turn: it's carried on as Codealong comes back, and the wait sees
        // that turn to its end, whether the cut shows as an error or, seen already, idle.
        for shown in [Status::Error, Status::Idle] {
            let mut w = Watch { carries_on: true, ..Watch::default() };
            assert_eq!(w.look(&s(Status::Working), 0, never), Seen::Going);
            assert_eq!(w.look(&cut(shown), 3_000, never), Seen::Going);
            assert_eq!(w.look(&cut(shown), 6_000, never), Seen::Going);
            assert_eq!(w.look(&s(Status::Working), 9_000, never), Seen::Going);
            assert_eq!(w.look(&s(Status::Unread), 12_000, never), Seen::Ended);
        }
        // Not carried on a few minutes later: it waits on its user, as an error does, and not as a
        // turn that ended, though it shows idle once seen.
        for shown in [Status::Error, Status::Idle] {
            let mut w = Watch { carries_on: true, ..Watch::default() };
            assert_eq!(w.look(&s(Status::Working), 0, never), Seen::Going);
            assert_eq!(w.look(&cut(shown), 3_000, never), Seen::Going);
            assert_eq!(w.look(&cut(shown), 3_000 + WAIT_CARRY_ON_MS, never), Seen::OnUser);
        }
        // Nor waited through with the setting off, or for one cut off in a terminal.
        assert_eq!(Watch::default().look(&cut(Status::Error), 0, never), Seen::OnUser);
        let terminal = Session { hosted: false, ..cut(Status::Error) };
        assert_eq!(Watch { carries_on: true, ..Watch::default() }.look(&terminal, 0, never), Seen::OnUser);
    }

    #[test]
    fn a_search_leaves_out_the_session_running_it() {
        let streets = |key: &str| Session { title: "Complete streets pilot".into(), ..session(key, true, None) };
        let b = Board { lanes: vec![lane("/r/main", vec![streets("me"), streets("other"), session("asked", true, None), session("quiet", true, None)])], ..Board::default() };
        let Cmd::Sessions(SessionsArgs { cmd: Some(SessionsCmd::Find { filter, .. }), .. }) = Cli::try_parse_from(["codealong", "sessions", "find", "streets"]).unwrap().cmd else { panic!() };
        // Its own conversation has the words too, as another's does.
        let said = HashMap::from([("me".to_string(), "…the streets pilot…".to_string()), ("asked".to_string(), "…streets…".to_string())]);
        let wanted = |me: Option<&str>| Wanted { status: None, worktree: None, since: None, words: vec!["streets".into()], said: said.clone(), me: me.map(String::from), now: 0 };
        let found = |w: Wanted| all_sessions(&b).into_iter().filter(|(s, l)| w.lists(&filter, s, l)).map(|(s, _)| s.key.clone()).collect::<Vec<_>>();
        assert_eq!(found(wanted(Some("me"))), ["other", "asked"]);
        // Run from no session it knows, it lists them all.
        assert_eq!(found(wanted(None)), ["me", "other", "asked"]);
    }

    #[test]
    fn a_search_puts_the_session_it_names_before_the_ones_that_mention_it() {
        let titled = |key: &str, title: &str, prompted: i64| Session { title: title.into(), last_prompt_at: prompted, ..session(key, false, None) };
        let b = Board {
            lanes: vec![lane(
                "/r/main",
                vec![
                    // Prompted most recently, so first without a search.
                    titled("quoted", "Fix the kestrel import", 90),
                    Session { excerpt: Some("Waiting on the harbor watch".into()), ..titled("excerpt", "Atlas map tiles", 80) },
                    Session { name: Some("Nightly harbor watch".into()), ..titled("by-name", "Release notes", 70) },
                    titled("longer", "Harbor watch for northwind", 60),
                    // The words split between its title and its name: its row has them.
                    Session { name: Some("Night watch".into()), ..titled("split", "Harbor cleanup", 75) },
                    titled("exact", "Harbor  Watch", 10),
                ],
            )],
            ..Board::default()
        };
        let Cmd::Sessions(SessionsArgs { cmd: Some(SessionsCmd::Find { filter, .. }), .. }) = Cli::try_parse_from(["codealong", "sessions", "find", "harbor", "watch"]).unwrap().cmd else { panic!() };
        let said = HashMap::from([("quoted".to_string(), "…ask the harbor watch first…".to_string()), ("exact".to_string(), "…harbor watch…".to_string())]);
        let wanted = |words: &[&str]| Wanted { status: None, worktree: None, since: None, words: words.iter().map(|w| w.to_string()).collect(), said: said.clone(), me: None, now: 0 };
        let order = |w: Wanted| {
            let mut found: Vec<(&Session, &Lane)> = all_sessions(&b).into_iter().filter(|(s, l)| w.lists(&filter, s, l)).collect();
            w.sort(&mut found);
            found.into_iter().map(|(s, _)| s.key.clone()).collect::<Vec<_>>()
        };
        // Its title, then the titles and names with the words in them, then a row that has them,
        // then what only mentions them, however long ago each was prompted.
        assert_eq!(order(wanted(&["harbor", "watch"])), ["exact", "by-name", "longer", "excerpt", "split", "quoted"]);
        // A name that is the words is as close as a title that is.
        assert_eq!(order(wanted(&["nightly", "harbor", "watch"]))[0], "by-name");
        // Without words, the listing's own order: the one last prompted first.
        assert_eq!(order(wanted(&[])), ["quoted", "excerpt", "split", "by-name", "longer", "exact"]);
    }

    #[test]
    fn headless_runs_are_listed_only_when_asked_for() {
        let b = Board { lanes: vec![lane("/r/main", vec![session("me", true, None), Session { headless: true, ..session("judge", true, None) }])], ..Board::default() };
        let filter = |args: &[&str]| {
            let Cmd::Sessions(SessionsArgs { filter, .. }) = Cli::try_parse_from([&["codealong", "sessions"], args].concat()).unwrap().cmd else { panic!() };
            filter
        };
        let wanted = Wanted { status: None, worktree: None, since: None, words: vec![], said: HashMap::new(), me: None, now: 0 };
        let found = |f: SessionFilter| all_sessions(&b).into_iter().filter(|(s, l)| wanted.lists(&f, s, l)).map(|(s, _)| s.key.clone()).collect::<Vec<_>>();
        assert_eq!(found(filter(&[])), ["me"]);
        assert_eq!(found(filter(&["--headless"])), ["me", "judge"]);
    }
}
