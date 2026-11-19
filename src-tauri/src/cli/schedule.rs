//! `codealong schedule` and `codealong orchestrator`: schedules (schedules/mod.rs) listed, added,
//! edited, run and fired, and the orchestrators, which are schedules of a kind
//! (`Schedule::orchestrates`).
//!
//! A change is an `Op` (`schedules::apply_op`), made from here; the server's scheduler runs what
//! the schedules then say, so with no server a change waits for one. `with_options` reads what
//! `schedule add` and `schedule edit` were given onto a schedule: each option given changes what
//! it names and the rest stay. `orchestrator_guard` turns down what an orchestrator's session may
//! not ask for, in the commands that ask without saying who does (a start, a fork, removing a
//! worktree): a new one of those calls it.

use anyhow::{anyhow, bail, Context, Result};
use clap::ValueEnum;
use serde::Serialize;
use serde_json::Value;

use crate::server::{self, Request};
use crate::start::lane_for_path;
use crate::util::{expand, now_ms, one_line, tilde};

use super::*;

// ---- schedule ----

pub(super) fn schedule(cmd: ScheduleCmd, c: &mut Client, json: bool) -> Result<()> {
    use crate::schedules::{self, rule::Local, Op};
    let now = now_ms();
    let op = match cmd {
        ScheduleCmd::List => {
            let items = schedule_items(c)?;
            if json {
                return print_json(&items);
            }
            let board = c.board()?.clone();
            let rows = items
                .iter()
                .map(|i| {
                    let next = match (i.finished, i.schedule.paused, i.next_at) {
                        (Some(_), _, _) => "finished".to_string(),
                        (_, true, _) => "paused".to_string(),
                        (_, _, Some(at)) => schedules::clock_text(at, &Local),
                        _ => String::new(),
                    };
                    let last = i.runs.first().map(|r| format!("{:?} {}", r.outcome, ago(now - r.at)).to_lowercase()).unwrap_or_default();
                    vec![i.schedule.name.clone(), i.when_words.clone(), schedules::does_words(&i.schedule, &board), next, i.problem.clone().unwrap_or(last)]
                })
                .collect();
            print!("{}", table_text(&["NAME", "WHEN", "DOES", "NEXT", "LAST RUN"], rows));
            return Ok(());
        }
        ScheduleCmd::Show { schedule } => {
            let items = schedule_items(c)?;
            let item = items.iter().find(|i| i.schedule.id == schedule.trim()).or_else(|| items.iter().find(|i| i.schedule.name.eq_ignore_ascii_case(schedule.trim()))).ok_or_else(|| anyhow!("There's no schedule named {schedule}. `codealong schedule list` lists them."))?;
            if json {
                return print_json(item);
            }
            let board = c.board()?.clone();
            println!("{} ({})", item.schedule.name, item.schedule.id);
            println!("When: {}{}{}", item.when_words, if item.schedule.once { ", then stops" } else { "" }, if item.schedule.paused { " (paused)" } else { "" });
            if let Some(at) = item.finished {
                println!("Finished: {}. Only `schedule run` runs it now.", schedules::clock_text(at, &Local));
            }
            println!("Does: {}", schedules::does_words(&item.schedule, &board));
            if let Some(words) = &item.runs_on_words {
                println!("Runs on: {words}{}", item.here.as_deref().map(|h| format!(" (not here: {h})")).unwrap_or_default());
            }
            if let Some(at) = item.next_at {
                println!("Next: {}", schedules::clock_text(at, &Local));
            }
            if let Some(p) = &item.problem {
                println!("Can't run: {p}");
            }
            if let Some(check) = &item.check {
                println!("Its watch: {} ({})", check.said, ago(now - check.at));
            }
            println!("Prompt: {}", item.schedule.run.prompt.trim());
            for r in &item.runs {
                let session = r.session.as_deref().map(|k| format!(" · {k}")).unwrap_or_default();
                let on = r.machine.as_deref().map(|m| format!(" · on {m}")).unwrap_or_default();
                println!("  {} · {} · {:?}{on}{session}{}", schedules::clock_text(r.at, &Local), r.why, r.outcome, if r.said.is_empty() { String::new() } else { format!(" · {}", one_line(&r.said, 120)) });
            }
            return Ok(());
        }
        ScheduleCmd::Times { rule } => {
            let times = schedules::times(&rule.join(" "), now, &Local);
            if json {
                return print_json(&times);
            }
            if let Some(p) = &times.problem {
                bail!("{p}");
            }
            println!("{}", times.words);
            for t in &times.next {
                println!("  {}", schedules::clock_text(*t, &Local));
            }
            return Ok(());
        }
        ScheduleCmd::Add(a) => {
            let mut o = a.options;
            if o.at.is_none() && o.watch.is_none() && o.on.is_none() && !o.when_fired {
                bail!("Say when it runs: --at <time>, --watch <command>, --on <event> or --when-fired.");
            }
            if !o.chat && o.worktree.is_none() {
                o.worktree = Some(".".into());
            }
            Op::Save { schedule: with_options(c, schedules::Schedule { name: a.name, ..schedules::Schedule::default() }, o)? }
        }
        ScheduleCmd::Edit(e) => {
            let was = schedules::load()?.find(&e.schedule).cloned().ok_or_else(|| anyhow!("There's no schedule named {}. `codealong schedule list` lists them.", e.schedule.trim()))?;
            let mut s = with_options(c, was.clone(), e.options)?;
            if let Some(name) = e.name {
                s.name = name;
            }
            // Saved as it was, it would only look changed to your other machines.
            if s == was {
                if json {
                    return print_json(&was);
                }
                println!("{} Nothing changed.", saved_line(c, &was.name, was.paused)?);
                return Ok(());
            }
            Op::Save { schedule: s }
        }
        ScheduleCmd::Remove { schedule } => Op::Remove { id: schedule },
        ScheduleCmd::Pause { schedule } => Op::Pause { id: schedule, paused: true },
        ScheduleCmd::Resume { schedule } => Op::Pause { id: schedule, paused: false },
        ScheduleCmd::Run { schedule } => Op::Run { id: schedule },
        ScheduleCmd::Fire(f) => {
            let text = match f.text.as_deref() {
                Some("-") => read_text(None, Some("-".into()))?.unwrap_or_default(),
                other => other.unwrap_or_default().to_string(),
            };
            Op::Fire { id: f.schedule, key: f.key.unwrap_or_default(), title: f.title.unwrap_or_default(), text }
        }
    };
    let asks_to_run = matches!(op, Op::Run { .. } | Op::Fire { .. });
    let by = match &op {
        Op::Save { schedule } if schedule.id.is_empty() => find_me(c)?.0,
        _ => None,
    };
    let cfg = c.cfg()?.clone();
    let board = c.board()?.clone();
    let done = schedules::apply_op(op, by.as_deref(), Some(&board), &cfg, now, &Local)?;
    if json {
        return print_json(&done);
    }
    let name = done.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
    match () {
        _ if asks_to_run && c.app.is_none() => println!("Asked. Codealong's server isn't running: it runs once the server starts."),
        _ if asks_to_run => println!("Asked: it runs within a few seconds. `codealong schedule show` has how it went."),
        _ if done.get("removed").is_some() => println!("Removed."),
        _ => {
            println!("{}", saved_line(c, &name, done.get("paused").and_then(Value::as_bool) == Some(true))?);
            if c.app.is_none() {
                println!("Codealong's server isn't running: schedules run in it.");
            }
        }
    }
    Ok(())
}

// ---- orchestrator ----

/// Turns down what an orchestrator's session may not ask for (schedules/orchestrate.rs
/// `refused_to`), where the command asks without naming the session running it, which actions.rs
/// would tell it by: a start, a fork, removing a worktree.
pub(super) fn orchestrator_guard(c: &mut Client, request: &str) -> Result<()> {
    let Some(me) = find_me(c)?.0 else { return Ok(()) };
    match crate::schedules::orchestrate::refused_to(c.board()?, &me, request) {
        Some(why) => bail!("{why}"),
        None => Ok(()),
    }
}

/// An orchestrator in a row of `orchestrator list`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OrchestratorRow {
    /// The project it keeps watch over; none for All projects.
    project: Option<String>,
    on: bool,
    /// Its schedule, by id, which `schedule show` takes.
    schedule: String,
    /// The session it keeps going; none before its first run, or once it's turned off.
    session: Option<String>,
    account: String,
    last_run: Option<crate::schedules::RunRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    problem: Option<String>,
}

pub(super) fn orchestrator(cmd: OrchestratorCmd, c: &mut Client, json: bool) -> Result<()> {
    use crate::schedules::{self, orchestrate, rule::Local, Op};
    let now = now_ms();
    let scope_of = |a: &OrchestratorScope| orchestrate::Orchestrates::of(a.project.as_deref());
    let start_hint = |scope: &orchestrate::Orchestrates| format!("`codealong orchestrator start{}`", scope.project.as_deref().map(|p| format!(" {p}")).unwrap_or_default());
    // The orchestrator a command names, as the Schedules view has it.
    let named = |c: &mut Client, scope: &orchestrate::Orchestrates| -> Result<schedules::Item> {
        schedule_items(c)?
            .into_iter()
            .find(|i| i.schedule.orchestrates.as_ref().is_some_and(|o| o.same(scope)))
            .ok_or_else(|| anyhow!("There's no orchestrator for {}. {} starts one.", scope.words(), start_hint(scope)))
    };
    let cfg = c.cfg()?.clone();
    match cmd {
        OrchestratorCmd::List => {
            let rows: Vec<OrchestratorRow> = schedule_items(c)?
                .into_iter()
                .filter_map(|i| {
                    let scope = i.schedule.orchestrates.clone()?;
                    let account = Some(i.schedule.run.account.clone()).filter(|a| !a.is_empty()).unwrap_or_else(|| "auto".into());
                    Some(OrchestratorRow { project: scope.project, on: orchestrate::on(&i.schedule), schedule: i.schedule.id.clone(), session: i.thread.clone(), account, last_run: i.runs.first().cloned(), problem: i.problem.clone() })
                })
                .collect();
            if json {
                return print_json(&rows);
            }
            if rows.is_empty() {
                println!("No orchestrators. `codealong orchestrator start [project]` starts one.");
                return Ok(());
            }
            let table = rows
                .iter()
                .map(|r| {
                    let last = r.last_run.as_ref().map(|x| format!("{} {}", word(&x.outcome), ago(now - x.at))).unwrap_or_default();
                    vec![r.project.clone().unwrap_or_else(|| orchestrate::ALL.into()), if r.on { "on".into() } else { "off".into() }, r.session.clone().unwrap_or_default(), r.account.clone(), r.problem.clone().unwrap_or(last)]
                })
                .collect();
            print!("{}", table_text(&["KEEPS WATCH OVER", "", "SESSION", "ACCOUNT", "LAST RUN"], table));
            Ok(())
        }
        OrchestratorCmd::Start(a) => {
            let scope = scope_of(&a.scope);
            let board = c.board()?.clone();
            let op = Op::Orchestrate { project: scope.project.clone(), account: a.account.unwrap_or_default(), model: a.model, send: a.may_send, start: a.may_start };
            let done = schedules::apply_op(op, None, Some(&board), &cfg, now, &Local)?;
            if json {
                return print_json(&done);
            }
            let added = done.get("added").and_then(Value::as_bool) == Some(true);
            println!("{} for {}: it takes its first look within a few seconds, then runs whenever something it keeps watch over needs a look.", if added { "Started an orchestrator" } else { "The orchestrator goes on" }, scope.words());
            if c.app.is_none() {
                println!("Codealong's server isn't running: it runs once the server starts.");
            }
            Ok(())
        }
        OrchestratorCmd::Stop(a) => {
            let scope = scope_of(&a);
            let item = named(c, &scope)?;
            let board = c.board()?.clone();
            // Paused, its scheduler marks its session done and drops what waited for it (runner.rs).
            let done = schedules::apply_op(Op::Pause { id: item.schedule.id.clone(), paused: true }, None, Some(&board), &cfg, now, &Local)?;
            if json {
                return print_json(&done);
            }
            println!("Turned off the orchestrator for {}: its session is marked done within a few seconds, and its notes are kept for when it's started again.", scope.words());
            if c.app.is_none() {
                println!("Codealong's server isn't running: its session is marked done once the server starts.");
            }
            Ok(())
        }
        OrchestratorCmd::Look(a) => {
            let scope = scope_of(&a);
            let item = named(c, &scope)?;
            if !orchestrate::on(&item.schedule) {
                bail!("The orchestrator for {} is off: {} starts it, and it looks at once.", scope.words(), start_hint(&scope));
            }
            let board = c.board()?.clone();
            let done = schedules::apply_op(Op::Run { id: item.schedule.id.clone() }, None, Some(&board), &cfg, now, &Local)?;
            if json {
                return print_json(&done);
            }
            println!("Asked: it looks within a few seconds, once its session's turn has ended.");
            Ok(())
        }
        OrchestratorCmd::Fresh(a) => {
            let scope = scope_of(&a);
            let item = named(c, &scope)?;
            if !orchestrate::on(&item.schedule) {
                bail!("The orchestrator for {} is off: {} starts it with a new session.", scope.words(), start_hint(&scope));
            }
            let board = c.board()?.clone();
            let done = schedules::apply_op(Op::Fresh { id: item.schedule.id.clone() }, None, Some(&board), &cfg, now, &Local)?;
            if json {
                return print_json(&done);
            }
            println!("Asked: a new session takes its first look within a few seconds, going on from its notes, and the one it had is marked done.");
            Ok(())
        }
    }
}

/// The schedules with what's happened to them, as the app's Schedules view has them: read from the
/// files, so a change just made is there, with where each your machines share runs and their runs
/// of it, which only the app knows, from the app's.
fn schedule_items(c: &mut Client) -> Result<Vec<crate::schedules::Item>> {
    let (file, runs) = (crate::schedules::load()?, crate::schedules::load_runs()?);
    let cfg = c.cfg()?.clone();
    let board = c.board()?;
    let mut items = crate::schedules::items(&file, &runs, board, &cfg, now_ms(), &crate::schedules::rule::Local);
    for item in items.iter_mut().filter(|i| i.schedule.runs_on.shared()) {
        if let Some(app) = board.schedules.iter().find(|a| a.schedule.id == item.schedule.id && a.schedule.updated_at == item.schedule.updated_at) {
            item.problem = app.problem.clone();
            item.here = app.here.clone();
            item.runs_on_words = app.runs_on_words.clone();
            item.runs = app.runs.clone();
        }
    }
    Ok(items)
}

/// A schedule just saved, in a line: when it runs, when next, and whether it's paused.
fn saved_line(c: &mut Client, name: &str, paused: bool) -> Result<String> {
    use crate::schedules::{clock_text, rule::Local};
    let item = schedule_items(c)?.into_iter().find(|i| i.schedule.name == name);
    let next = item.as_ref().and_then(|i| i.next_at).map(|at| format!(" Next: {}.", clock_text(at, &Local))).unwrap_or_default();
    let paused = if paused { " It's paused." } else { "" };
    let once = match &item {
        Some(i) if i.finished.is_some() => " It has run, and is finished.",
        Some(i) if i.schedule.once => " It stops after its first run.",
        _ => "",
    };
    Ok(format!("{name}: {}.{next}{paused}{once}", item.map(|i| i.when_words).unwrap_or_default()))
}

/// A schedule with what `schedule add` or `schedule edit` said of it: each option given changes
/// what it names, and the rest stay as `s` has them (a new schedule's, for add).
fn with_options(c: &mut Client, mut s: crate::schedules::Schedule, o: ScheduleOptions) -> Result<crate::schedules::Schedule> {
    use crate::schedules::{CatchUp, Event, Overlap, RunsOn, To, WatchMode, When};
    // When it runs: what it's said to run at or on takes the place of what it ran at or on, a
    // watch keeping how often it looks and an event its filters while it's still that event.
    if let Some(rule) = o.at {
        s.when = When::Time { rule };
    }
    if let Some(command) = o.watch {
        s.when = match std::mem::take(&mut s.when) {
            When::Watch { every, mode, .. } => When::Watch { command, every, mode },
            _ => When::Watch { command, every: "5m".into(), mode: WatchMode::Lines },
        };
    }
    if let Some(on) = o.on {
        let event = match on {
            ScheduleEventArg::PrMerged => Event::PrMerged,
            ScheduleEventArg::ChecksFailed => Event::ChecksFailed,
            ScheduleEventArg::TodoReady => Event::TodoReady,
        };
        s.when = match std::mem::take(&mut s.when) {
            When::Event { event: was, repo, label, mine } if was == event => When::Event { event, repo, label, mine },
            _ => When::Event { event, repo: String::new(), label: String::new(), mine: false },
        };
    }
    if o.when_fired {
        s.when = When::Fired;
    }
    match &mut s.when {
        When::Watch { every, mode, .. } => {
            if let Some(e) = o.every {
                *every = e;
            }
            if let Some(v) = o.value {
                *mode = if v { WatchMode::Value } else { WatchMode::Lines };
            }
        }
        _ if o.every.is_some() || o.value.is_some() => bail!("--every and --value are for a schedule that runs on a watch (--watch)."),
        _ => {}
    }
    match &mut s.when {
        When::Event { repo, label, mine, .. } => {
            if let Some(r) = o.repo {
                *repo = r.trim().to_string();
            }
            if let Some(l) = o.label {
                *label = l.trim().to_string();
            }
            if let Some(m) = o.mine {
                *mine = m;
            }
        }
        _ if o.repo.is_some() || o.label.is_some() || o.mine.is_some() => bail!("--repo, --mine and --label are for a schedule that runs on an event (--on)."),
        _ => {}
    }
    if let Some(prompt) = read_text(o.prompt, o.prompt_file)? {
        s.run.prompt = prompt;
    }
    let was_to = s.run.to;
    match (o.to.as_deref().map(str::trim), o.thread) {
        (Some("me"), _) => match find_me(c)?.0 {
            Some(me) => (s.run.to, s.run.session) = (To::Session, me),
            None => bail!("No session runs this command, so there's no `me` to send to. Give a session's key."),
        },
        (Some(key), _) => {
            let key = session_in(key)?;
            let board = c.loaded_board()?;
            if !all_sessions(board).iter().any(|(x, _)| x.key == key) {
                return Err(no_session(board, key));
            }
            (s.run.to, s.run.session) = (To::Session, key.to_string());
        }
        (None, Some(thread)) => (s.run.to, s.run.session) = (if thread { To::Thread } else { To::New }, String::new()),
        (None, None) => {}
    }
    if let Some(after) = o.fresh_after {
        if s.run.to != To::Thread {
            bail!("--fresh-after is for a schedule that keeps one session going (--thread).");
        }
        s.run.fresh_after = after.trim().to_string();
    }
    // Where its sessions start, and where its watch and its check run: a schedule that sends to a
    // session keeps its worktree for those, and does without one outside any.
    if o.chat {
        (s.run.folder, s.run.worktree) = (String::new(), false);
    } else if let Some(worktree) = o.worktree {
        let dir = dunce::canonicalize(expand(&worktree)).unwrap_or_else(|_| expand(&worktree));
        s.run.folder = match lane_for_path(c.board()?, &dir) {
            Some(lane) => lane.path.clone(),
            None if s.run.to == To::Session => String::new(),
            None => bail!("{} isn't in a worktree Codealong knows. Give one with --worktree, or --chat for no project.", tilde(&dir)),
        };
    } else if was_to == To::Session && s.run.to != To::Session && s.run.folder.is_empty() {
        // One that sent to a session outside any worktree would start chats from now on.
        bail!("Say where its sessions start: --worktree <path>, or --chat for no project.");
    }
    if let Some(fresh) = o.new_worktree {
        s.run.worktree = fresh;
    }
    if s.run.worktree && s.run.folder.is_empty() && s.run.to != To::Session {
        bail!("A chat has no repo to make a worktree of: give --worktree too.");
    }
    if let Some(account) = o.account {
        s.run.account = if account == "auto" { String::new() } else { account };
    }
    if let Some(model) = o.model {
        s.run.model = Some(model).filter(|m| !m.trim().is_empty());
    }
    if let Some(effort) = o.effort {
        s.run.effort = Some(effort).filter(|e| !e.is_empty());
    }
    if let Some(place) = o.place {
        s.run.place = match place.as_str() {
            "" => None,
            p => Some(place_of(PlaceArg::from_str(p, false).map_err(|e| anyhow!(e))?)),
        };
    }
    if let Some(only_if) = o.only_if {
        s.only_if = only_if;
    }
    if let Some(quiet) = o.quiet {
        s.quiet = quiet;
    }
    if let Some(notes) = o.notes {
        s.notes = notes;
    }
    if let Some(allow) = o.allow_overlap {
        s.overlap = if allow { Overlap::Allow } else { Overlap::Skip };
    }
    if let Some(skip) = o.skip_missed {
        s.catch_up = if skip { CatchUp::Skip } else { CatchUp::Once };
    }
    if let Some(n) = o.per_day {
        s.per_day = match n.trim() {
            "" => None,
            n => Some(n.parse().map_err(|_| anyhow!("Couldn't read “{n}” as how many runs a day: give a number, or \"\" for no limit."))?),
        };
    }
    if let Some(once) = o.once {
        s.once = once;
    }
    if let Some(paused) = o.paused {
        s.paused = paused;
    }
    // Which machines run it: --prefer names one for it to go to, while it runs on one of them.
    let one = o.runs_on.map_or(matches!(s.runs_on, RunsOn::One { .. }), |r| r == RunsOnArg::One);
    if o.prefer.is_some() && !one {
        bail!("--prefer is for --runs-on one.");
    }
    s.runs_on = match (o.runs_on, o.prefer) {
        (Some(RunsOnArg::This), _) => RunsOn::This,
        (Some(RunsOnArg::Every), _) => RunsOn::Every,
        (_, Some(p)) if p.trim().is_empty() => RunsOn::One { prefer: String::new() },
        (_, Some(p)) => RunsOn::One { prefer: preferred_machine(&p)? },
        (Some(RunsOnArg::One), None) => match std::mem::take(&mut s.runs_on) {
            RunsOn::One { prefer } => RunsOn::One { prefer },
            _ => RunsOn::One { prefer: String::new() },
        },
        (None, None) => std::mem::take(&mut s.runs_on),
    };
    Ok(s)
}

/// The device key of the machine `--prefer` names: one of your linked machines, or this one.
fn preferred_machine(named: &str) -> Result<String> {
    let view: server::DevicesSummary = serde_json::from_value(ask_app(&Request::Devices)?).context("reading the app's devices")?;
    if !view.enabled {
        bail!("Your devices are off: turn them on in Settings › Devices, and link your other machines, for a schedule to run on them.");
    }
    if named.trim().eq_ignore_ascii_case("this") || named.trim().eq_ignore_ascii_case(&view.me.name) {
        return Ok(view.me.key);
    }
    Ok(which_device(&view, named, &[Listed::Linked])?.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::lane;
    use crate::model::Board;
    use crate::start::Place;

    /// An agent asked to move a daily schedule's time removed it and added it again, twice,
    /// which gave it a new id and lost its runs and notes (2026-10-31): `schedule edit` changes
    /// what it's given and keeps the rest.
    #[test]
    fn a_schedule_is_edited_in_place() {
        use crate::schedules::{CatchUp, Event, Run, Schedule, To, WatchMode, When};
        let parse = |a: &[&str]| Cli::try_parse_from(["codealong", "schedule"].into_iter().chain(a.iter().copied())).map(|c| c.cmd);
        let edit = |a: &[&str]| match parse(&[&["edit", "standup"], a].concat()) {
            Ok(Cmd::Schedule { cmd: ScheduleCmd::Edit(e) }) => e,
            other => panic!("{other:?}"),
        };
        let board = Board { lanes: vec![lane("/work/atlas", vec![crate::fixtures::session("claude:claude:kestrel")]), lane("/work/northwind", vec![])], ..Board::default() };
        let mut c = Client { app: None, verbose: false, cfg: None, board: Some(board), from_app: false, started: false, said: false, me: Some((Some("claude:claude:kestrel".into()), Some("environment"))), elsewhere: None, veiled: None };
        let daily = Schedule {
            id: "s-1".into(),
            name: "standup".into(),
            when: When::Time { rule: "daily 15:00".into() },
            run: Run { to: To::Thread, folder: "/work/atlas".into(), account: "work".into(), model: Some("opus".into()), prompt: "Sum up the day".into(), ..Run::default() },
            quiet: true,
            per_day: Some(2),
            created_at: 5,
            ..Schedule::default()
        };
        let mut with = |s: &Schedule, a: &[&str]| with_options(&mut c, s.clone(), edit(a).options);

        // Its time alone changes.
        let moved = with(&daily, &["--at", "daily 14:00"]).unwrap();
        assert_eq!(moved, Schedule { when: When::Time { rule: "daily 14:00".into() }, ..daily.clone() });
        // Nothing given changes nothing.
        assert_eq!(with(&daily, &[]).unwrap(), daily);
        // A yes-or-no one goes off with `=no`, and a text one comes off when it's empty.
        let off = with(&daily, &["--quiet=no", "--model", "", "--per-day", "", "--thread=no", "--skip-missed"]).unwrap();
        assert_eq!((off.quiet, off.run.model, off.per_day, off.run.to, off.catch_up), (false, None, None, To::New, CatchUp::Skip));
        let placed = with(&daily, &["--effort", "high", "--in", "terminal"]).unwrap();
        assert_eq!((placed.run.effort.as_deref(), placed.run.place), (Some("high"), Some(Place::Terminal)));
        let unplaced = with(&placed, &["--effort", "", "--in", ""]).unwrap();
        assert_eq!((unplaced.run.effort, unplaced.run.place), (None, None));
        assert!(parse(&["edit", "standup", "--in", "somewhere"]).is_err());
        assert!(parse(&["edit", "standup", "--quiet", "no"]).is_err(), "`no` isn't taken for the schedule's name");
        assert!(with(&daily, &["--per-day", "twice"]).unwrap_err().to_string().contains("how many runs a day"));
        // Sent to its session from now on, then on to another worktree.
        let mine = with(&daily, &["--to", "me"]).unwrap();
        assert_eq!((mine.run.to, mine.run.session.as_str(), mine.run.folder.as_str()), (To::Session, "claude:claude:kestrel", "/work/atlas"));
        assert_eq!(with(&daily, &["--worktree", "/work/northwind/src"]).unwrap().run.folder, "/work/northwind");
        assert!(with(&daily, &["--to", "claude:claude:nobody"]).is_err());
        // A session's link sends to the session it names, and one that names none to nobody.
        assert_eq!(with(&daily, &["--to", "codealong://session/claude:claude:kestrel"]).unwrap().run.session, "claude:claude:kestrel");
        assert!(with(&daily, &["--to", "codealong://session/"]).is_err());
        // One that sends to a session outside any worktree is told where new ones start.
        let outside = Schedule { run: Run { to: To::Session, session: "claude:claude:kestrel".into(), folder: String::new(), ..daily.run.clone() }, ..daily.clone() };
        assert!(with(&outside, &["--thread"]).unwrap_err().to_string().contains("--worktree"));
        assert_eq!(with(&outside, &["--thread", "--chat"]).unwrap().run.to, To::Thread);

        // A watch keeps how often it looks through a new command, and changes it alone.
        let watch = Schedule { when: When::Watch { command: "./deploys".into(), every: "10m".into(), mode: WatchMode::Lines }, ..daily.clone() };
        assert_eq!(with(&watch, &["--watch", "./releases"]).unwrap().when, When::Watch { command: "./releases".into(), every: "10m".into(), mode: WatchMode::Lines });
        assert_eq!(with(&watch, &["--every", "1h", "--value"]).unwrap().when, When::Watch { command: "./deploys".into(), every: "1h".into(), mode: WatchMode::Value });
        assert!(with(&daily, &["--every", "1h"]).unwrap_err().to_string().contains("--watch"));
        // An event keeps its filters while it's that event, and starts without them as another.
        let merged = with(&daily, &["--on", "pr-merged", "--repo", "northwind/web"]).unwrap();
        assert_eq!(merged.when, When::Event { event: Event::PrMerged, repo: "northwind/web".into(), label: String::new(), mine: false });
        assert_eq!(with(&merged, &["--mine"]).unwrap().when, When::Event { event: Event::PrMerged, repo: "northwind/web".into(), label: String::new(), mine: true });
        assert_eq!(with(&merged, &["--on", "todo-ready"]).unwrap().when, When::Event { event: Event::TodoReady, repo: String::new(), label: String::new(), mine: false });
        assert!(with(&daily, &["--label", "bug"]).unwrap_err().to_string().contains("--on"));
        assert!(with(&daily, &["--fresh-after", "1d", "--thread=no"]).is_err());
        assert!(parse(&["edit", "standup", "--at", "daily 9:00", "--when-fired"]).is_err());

        // Add's options read as they did, before its name too.
        let Ok(Cmd::Schedule { cmd: ScheduleCmd::Add(a) }) = parse(&["add", "--quiet", "triage", "--at", "weekdays 9:00", "--thread", "--prompt", "-x"]) else { panic!() };
        assert_eq!((a.name.as_str(), a.options.quiet, a.options.thread, a.options.prompt.as_deref()), ("triage", Some(true), Some(true), Some("-x")));
    }
}
