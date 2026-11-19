//! `codealong start`: a session started on a prompt or on todos, as the app's New session starts
//! one (start.rs picks the account, the place and the model), on this Mac or on another of your
//! devices (`--device`), which picks for itself.
//!
//! The file isn't `start.rs` because `start` in cli.rs names the engine's module.

use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;

use crate::actions::{self, params};
use crate::api;
use crate::server::{self, Request};
use crate::start::{self, Place, StartSpec};
use crate::todos::{self, Todo, TodoOp};
use crate::util::{self, now_ms, tilde};

use super::*;

// ---- start ----

pub(super) fn place_of(p: PlaceArg) -> Place {
    match p {
        PlaceArg::Terminal => Place::Terminal,
        PlaceArg::Background => Place::Background,
        PlaceArg::ClaudeApp => Place::ClaudeApp,
        PlaceArg::CodexApp => Place::CodexApp,
        PlaceArg::Codealong => Place::Codealong,
    }
}

pub(super) fn start_cmd(c: &mut Client, a: &StartArgs, json: bool) -> Result<()> {
    let given = read_text(a.prompt.clone(), a.prompt_file.clone())?;
    if let Some(device) = &a.device {
        return start_on_device(c, a, given, device, json);
    }
    // Codealong's chats are a lane like any other, by their folder, which start.rs gives a chat
    // of its own.
    let lane_id = if a.chat { crate::config::chats_dir().to_string_lossy().into_owned() } else { resolve_worktree(c, &a.worktree)? };
    // Run by an agent, the new session was started by that agent's.
    let from = find_me(c)?.0;
    orchestrator_guard(c, api::START)?;
    if a.helper && from.is_none() {
        bail!("--helper starts a helper of the session running this command, and this command isn't running in one. Start it without --helper: it's then a session of your own.");
    }
    // The todos whose primary session runs, which --take takes them from.
    let mut taken: Vec<(String, todos::Primary)> = Vec::new();
    let (prompt, todo_ids) = match (a.todo.as_slice(), given) {
        ([], Some(p)) => (p, Vec::new()),
        ([], None) => bail!("Give --todo, --prompt or --prompt-file."),
        (given, _) => {
            let store = c.todos()?;
            let mut ids: Vec<String> = Vec::new();
            for id in given {
                let id = resolve_todo(&store, id)?;
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            let ts: Vec<&Todo> = ids.iter().filter_map(|id| store.todos.iter().find(|t| &t.id == id)).collect();
            // A session that's a todo's primary one starts others to work alongside it; any other
            // agent's start would be a second one doing the same work.
            let mut turned_down = Vec::new();
            for t in &ts {
                let Some(p) = primary_elsewhere(c, t, from.as_deref())? else { continue };
                let who = session_words(c, &p.session);
                match (a.take, &from) {
                    (true, _) => taken.push((t.id.clone(), p)),
                    (false, Some(_)) => turned_down.push(format!("{}'s primary session is {who}, which is running. Send it what you have with `codealong session send {} \"…\"` rather than start another session on it; --take starts one that takes it over, for when your user asks.", t.id, p.session)),
                    (false, None) => eprintln!("note: {}'s primary session is {who}, which is running; the new session works alongside it. --take makes the new one the primary session.", t.id),
                }
            }
            if !turned_down.is_empty() {
                bail!("{}", turned_down.join("\n"));
            }
            for id in &ids {
                warn_blocked(&store, id, &ids);
            }
            let lanes = todos::LaneIndex::of(&c.board()?.lanes);
            let now = now_ms();
            // Taken over, its primary session is the one that had it before.
            let holding = |t: &Todo| !taken.iter().any(|(id, _)| id == &t.id) && t.primary.as_ref().is_some_and(|p| todos::holds(p, &lanes, now));
            (todos::prompt_for(&ts, holding), ids)
        }
    };
    let cfg = c.cfg()?.clone();
    // A helper of a boundary group's session starts inside the group unless --account says
    // where (boundary.rs): what its session hands it is the group's.
    let within = match (&from, a.helper) {
        (Some(k), true) => {
            let account = c.raw_board()?.session(k).map(|(_, s)| s.account.clone()).or_else(|| crate::boundary::account_of_key(k).map(String::from));
            account.and_then(|a| crate::boundary::group_of(&cfg, &a).cloned())
        }
        _ => None,
    };
    // Auto starts from the group the worktree's project names as its default.
    let project = start::project_at(c.raw_board()?, &lane_id).map(str::to_string);
    // Auto and the routing groups pick by the board's usage, which only the app's board has.
    let board = match c.app.is_some() {
        true => Some(c.board()?),
        false => None,
    };
    let account = match &within {
        Some(g) if a.account == "auto" => g.name.trim().to_string(),
        Some(g) if !crate::boundary::stays_inside(&cfg, g, &a.account) => {
            eprintln!("note: {} may run outside the boundary group {} this session is in, so what you send it leaves the group.", a.account, g.name.trim());
            a.account.clone()
        }
        _ => a.account.clone(),
    };
    let picked = start::pick(&cfg, board, &account, project.as_deref(), a.place.map(place_of), a.model.as_deref(), a.effort.as_deref(), a.helper, now_ms(), &crate::util::home())?;
    let spec = StartSpec { lane_id, account: picked.account, place: picked.place, prompt, model: picked.model, effort: picked.effort, worktree: a.new_worktree, todos: todo_ids, from, helper: a.helper, report: a.report };
    if a.dry_run {
        let account = cfg.accounts.iter().find(|x| x.name == spec.account).ok_or_else(|| anyhow!("There's no account named {}.", spec.account))?;
        let plan = start::plan_now(&cfg, account, spec.place, Path::new(&spec.lane_id), &spec.prompt, spec.model.as_deref(), spec.effort.as_deref(), spec.worktree)?;
        if json {
            return print_json(&serde_json::json!({ "spec": spec, "plan": plan }));
        }
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(());
    }
    // Without a primary session, the todo's next one becomes it: the one this starts.
    for (id, _) in &taken {
        c.apply(TodoOp::Primary { id: id.clone(), session: None })?;
    }
    let started = match c.start(&spec) {
        Ok(s) => s,
        Err(e) => {
            for (id, p) in taken {
                if let Err(e) = c.apply(TodoOp::Primary { id: id.clone(), session: Some(p.session) }) {
                    eprintln!("note: {id} was left without a primary session: {e:#}");
                }
            }
            return Err(e);
        }
    };
    if json {
        return print_json(&started);
    }
    println!("{}", started_line(&started, ""));
    Ok(())
}

/// What `start` prints for a session it started (`on` names the device, when it's another): its
/// key, and what to do next when there's something, as when Codex may not have taken a start's
/// prompt and it's for you to look.
fn started_line(started: &start::Started, on: &str) -> String {
    match (&started.key, &started.message) {
        (Some(k), Some(m)) => format!("Started {k}{on}. {m}"),
        (Some(k), None) => format!("Started {k}{on}."),
        (None, Some(m)) => m.clone(),
        (None, None) => format!("Started{on}."),
    }
}

/// `start --device`: the device picks where the session starts and on what, as `codealong start`
/// run there would (`api::START_PICK`), and then starts it as it does a window's start from here
/// (actions.rs `start_there`). Only through the app, which holds the link, so neither request can
/// end up started on this Mac.
fn start_on_device(c: &mut Client, a: &StartArgs, prompt: Option<String>, device: &str, json: bool) -> Result<()> {
    let prompt = prompt.ok_or_else(|| anyhow!("Give --prompt or --prompt-file."))?;
    if c.app.is_none() {
        bail!("Starting a session on another device needs Codealong's server, which couldn't be started here (`codealong server start` says why).");
    }
    let view: server::DevicesSummary = serde_json::from_value(ask_app(&Request::Devices)?).context("reading the app's devices")?;
    if !view.enabled {
        bail!("Devices is off on this device: turn it on in Settings › Devices, or [devices] enabled = true in config.toml.");
    }
    let (key, name) = which_device(&view, device, &[Listed::Linked])?;
    let short: String = key.chars().take(10).collect();
    let (folder, repo) = match a.worktree.as_str() {
        _ if a.chat => (String::new(), String::new()),
        // The device finds its own checkout of this repo.
        "." => {
            let here = std::env::current_dir()?;
            let repo = start::lane_at(c.board()?, &here).and_then(|l| l.repo_slug_now.clone().or_else(|| l.repo_slug.clone()));
            (String::new(), repo.ok_or_else(|| anyhow!("This folder isn't in a GitHub repo the board knows, so there's no checkout of it to find on {name}. Give --worktree: a folder there."))?)
        }
        // Under your home here, it's the same folder under that device's home.
        w => (tilde(Path::new(&util::resolve(&std::env::current_dir()?.to_string_lossy(), w))), String::new()),
    };
    let asked = params::StartPick { folder, repo, chat: a.chat, account: a.account.clone(), place: a.place.map(place_of), model: a.model.clone(), effort: a.effort.clone() };
    let from = find_me(c)?.0;
    let ask = |method: &str, at: &str, params: Value| ask_app(&Request::Run { name: method.into(), key: crate::peerfeed::remote_key(&short, at), params, from: from.clone() }).map_err(|e| device_said(e, &name));
    let mut spec: StartSpec = serde_json::from_value(ask(api::START_PICK, "", serde_json::to_value(&asked)?)?).context("reading where the device would start it")?;
    spec.prompt = prompt;
    spec.worktree = a.new_worktree;
    if a.dry_run {
        if json {
            return print_json(&serde_json::json!({ "device": name, "spec": spec }));
        }
        println!("On {name}: {}", serde_json::to_string_pretty(&spec)?);
        return Ok(());
    }
    let lane = spec.lane_id.clone();
    let started: start::Started = serde_json::from_value(ask(api::START, &lane, serde_json::to_value(params::Start { spec })?)?).context("reading the device's reply")?;
    if json {
        return print_json(&started);
    }
    println!("{}", started_line(&started, &format!(" on {name}")));
    Ok(())
}

/// What the app said of a request it put to the device `name`: a device or an app from before the
/// request said to be (`actions::UNKNOWN`), and the device's own words said to be its.
fn device_said(e: anyhow::Error, name: &str) -> anyhow::Error {
    let e = e.to_string();
    // The app here answers so when it doesn't know the request (actions.rs `asked`), the device
    // with "Codealong on this device …".
    if e.starts_with(server::UNKNOWN_REQUEST) || e.starts_with(&format!("Codealong {}", actions::UNKNOWN)) {
        return anyhow!("The Codealong that's running is older than this command and can't start a session on another device. Update it, or quit it and open it again.");
    }
    if e.contains(actions::UNKNOWN) {
        return anyhow!("Codealong on {name} is from before it could be asked to start a session by the command: update it there.");
    }
    // What the link says of the device names it already (peers.rs: not connected, remote control off).
    match e.contains(name) {
        true => anyhow!("{e}"),
        false => anyhow!("{name}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A start on another Mac is that Mac's: its todos and helpers are this one's, so neither goes
    /// with it, and what it says back is said to be its.
    #[test]
    fn a_start_on_another_device_takes_a_prompt_and_says_whose_words_are_whose() {
        let Cmd::Start(s) = Cli::try_parse_from(["codealong", "start", "--device", "mini", "--prompt", "Fix it", "--worktree", "~/projects/northwind", "--in", "background"]).unwrap().cmd else { panic!() };
        assert_eq!((s.device.as_deref(), s.worktree.as_str(), s.place), (Some("mini"), "~/projects/northwind", Some(PlaceArg::Background)));
        assert!(Cli::try_parse_from(["codealong", "start", "--device", "mini", "--chat", "--prompt", "What's new?"]).is_ok());
        for other in [&["--todo", "t-a"][..], &["--helper", "--prompt", "Review it"], &["--todo", "t-a", "--take"]] {
            let line: Vec<&str> = ["codealong", "start", "--device", "mini"].into_iter().chain(other.iter().copied()).collect();
            assert!(Cli::try_parse_from(line).is_err(), "{other:?}");
        }
        let said = |e: &str| device_said(anyhow!("{e}"), "Mini").to_string();
        assert_eq!(said("There's no account named claude9."), "Mini: There's no account named claude9.");
        assert_eq!(said("Remote control is off on Mini; turn it on there in Settings › Devices."), "Remote control is off on Mini; turn it on there in Settings › Devices.");
        assert_eq!(said("Codealong on this device doesn't know how to start-pick yet; update it here."), "Codealong on Mini is from before it could be asked to start a session by the command: update it there.");
        assert!(said("Codealong doesn't know how to start-pick.").starts_with("The Codealong that's running is older than this command"));
        assert!(said(&format!("{}: unknown variant `run`", server::UNKNOWN_REQUEST)).starts_with("The Codealong that's running is older than this command"));
    }

    /// A start says the session it started and, when there's something to do next, that too: a
    /// Codex start whose turn may not have gone named only its session.
    #[test]
    fn a_start_says_its_session_and_what_to_do_next() {
        let started = |key: Option<&str>, message: Option<&str>| start::Started { key: key.map(String::from), message: message.map(String::from) };
        let look = "Codealong asked Codex to start a turn with this and didn't hear how it went: look there before sending it again.";
        assert_eq!(started_line(&started(Some("codex:codex:t1"), Some(look)), ""), format!("Started codex:codex:t1. {look}"));
        assert_eq!(started_line(&started(Some("codex:codex:t1"), None), " on Mini"), "Started codex:codex:t1 on Mini.");
        assert_eq!(started_line(&started(None, Some("A new Codex thread is open in northwind.")), ""), "A new Codex thread is open in northwind.");
        assert_eq!(started_line(&started(None, None), " on Mini"), "Started on Mini.");
    }
}
