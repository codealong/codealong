//! Routing for the quick panel: what you type goes to TypeSafe's Jev, which answers a few typed
//! questions about it at once (docs.typesafe.ai): is it for a session that's already going, new
//! work, a search, or a note for later; which session; which worktree; and does it want a
//! worktree of its own. Jev picks from lists and gives a probability for every option, so the
//! code here decides what to suggest and how sure to sound, and the panel only acts on ⏎.
//!
//! Jev reads questions literally and is weak at dates and numbers, so times go in as words
//! ("5 minutes ago") and each question asks one plain thing.
//!
//! The request goes through jev.rs, which holds the key. Images pasted into what you type go
//! with it, each where it was in the text (`jev::Images`).
//!
//! Jev sees each session as a card: its title, where it is, its status and its latest line. A
//! message often names what was said in a conversation rather than its title ("the backoff we
//! found"), so the sessions whose prompts and replies share the most telling of its words
//! (search.rs `related`) come with the lines that share them, and come though they stopped too
//! long ago to be offered otherwise.
//!
//! When the accounts that can start a session don't all reach the same services through MCP
//! servers and connectors (mcp.rs: Gmail on one, not on another), Jev is also asked which one the
//! message needs, so "what does my email say" starts on an account that has Gmail.

use std::collections::{BTreeMap, HashSet};
use std::time::Instant;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use crate::board::ELSEWHERE;
use crate::model::{Board, Lane, Origin, Reuse, Session, Status};

/// "All projects" in the panel: every lane but the hidden ones.
pub const ALL_PROJECTS: &str = "All projects";

/// Sessions offered to Jev at most: live ones first, then the most recently prompted.
const MAX_SESSIONS: usize = 60;
/// Worktrees offered at most, the most recently active first.
const MAX_LANES: usize = 120;
/// Sessions that aren't running are offered only if you prompted them this recently.
const STOPPED_WITHIN_MS: i64 = 3 * 24 * 3600 * 1000;
/// How much of a session's title, prompt or doing line goes in.
const TEXT_CHARS: usize = 240;
/// Sessions whose conversations share words with the message that are looked for, in every
/// project, and how many in the panel's go to Jev with their lines.
const RELATED_LOOKED: usize = 40;
const RELATED: usize = 12;

pub use codealong_peer::api::{Action, AgentPick, NeedPick, Pick, Route};

/// What goes to Jev, and how to read its answer back: option keys (`s1`, `w1`) stand for
/// session keys and lane ids, so long paths never reach the model as labels.
#[derive(Debug)]
pub struct Request {
    pub body: Value,
    sessions: Vec<String>,
    /// Which of `sessions` are running.
    running: Vec<bool>,
    lanes: Vec<String>,
    lane_repos: Vec<String>,
    /// The repos what's typed names (`named_repos`), by `<project>/<repo>`.
    named: Vec<String>,
    /// The images pasted into what's typed, sent with it (`route`).
    images: crate::jev::Images,
    /// The `(account, model)` options offered for the `agent` question (`a1`, `a2`, …).
    agents: Vec<(String, Option<String>)>,
    /// The sessions that went with lines of their conversation sharing the message's words.
    pub related: Vec<String>,
    /// The services offered for the `needs` question (`c1`, `c2`, …).
    services: Vec<Service>,
}

/// A service the accounts' agents reach through an MCP server or connector, as the `needs`
/// question offers it.
#[derive(Debug, Clone, PartialEq)]
struct Service {
    name: String,
    tools: Vec<String>,
    /// The accounts that reach it, in the order they're given.
    accounts: Vec<String>,
    /// The model Auto would start the first of them on (`service_candidates`).
    model: Option<String>,
}

/// Jev's pick of a service counts when it's at least this sure, and surer than of none. Messages
/// about mail, chat and calendars came back 0.95 and more; "fix the flaky test" once gave GitHub
/// 0.63 (2026-10-20), and code work started on the one account with a GitHub app would be a
/// worse start, not a better one.
const NEEDS_AT: f64 = 0.7;
/// Tool names that go in to say what a service does, at most.
const SERVICE_TOOLS: usize = 12;

/// Sessions that could already be doing the work: running, and not waiting on you.
fn could_be_doing(s: &Session) -> bool {
    s.live && matches!(s.status, Status::Working | Status::Unread | Status::Idle)
}

/// A duplicate is only worth a warning when Jev is at least this sure.
const DUPLICATE_AT: f64 = 0.5;
/// Sessions Jev gives less than this are left out of the ranking.
const SCORED_AT: f64 = 0.01;
/// Scored sessions kept at most.
const SCORED: usize = 20;
/// How a session that isn't running counts against running ones Jev also picked. Sending to a
/// running session reaches it where it is; one that isn't running has to be resumed, a process
/// and often a terminal of its own, so when they're close the running one leads.
const STOPPED_WEIGHT: f64 = 0.5;

/// Whether a lane is in the panel's project, as the panel decides it (quick.ts `inProject`).
pub(crate) fn in_project(lane: &Lane, project: &str) -> bool {
    if project == ALL_PROJECTS {
        return !lane.hidden;
    }
    lane.project == project && (project == ELSEWHERE || !lane.hidden)
}

fn status_words(s: &Session) -> &'static str {
    match s.status {
        Status::Needs if s.asks => "finished its turn asking the user a question; what the user sends it next is the answer",
        Status::Needs => "waiting on the user to answer a question or approve something",
        Status::Error => "stopped on an error",
        Status::Working => "working on a turn right now",
        Status::Unread => "finished its turn; the user hasn't read the reply yet",
        Status::Idle => "idle, waiting for the user's next message",
        Status::Stopped => "not running; it can be resumed",
    }
}

fn clip(s: &str) -> String {
    crate::jev::clip(s, TEXT_CHARS)
}

/// A lane's own line: `callsign`, or with its repo when the name alone doesn't say.
fn lane_name(l: &Lane) -> String {
    if l.origin == Origin::Main {
        format!("{} (main checkout of {})", l.callsign, l.repo)
    } else if l.callsign == l.repo {
        l.callsign.clone()
    } else {
        format!("{} (worktree of {})", l.callsign, l.repo)
    }
}

fn session_card(lane: &Lane, s: &Session, related: Option<&crate::search::Related>, now: i64) -> Value {
    let mut card = Map::new();
    card.insert("title".into(), json!(clip(&s.title)));
    card.insert("agent".into(), json!(s.account));
    card.insert("project".into(), json!(lane.project));
    card.insert("repo".into(), json!(lane.repo));
    card.insert("worktree".into(), json!(lane_name(lane)));
    if let Some(b) = s.branch.as_deref().or(lane.branch.as_deref()) {
        card.insert("branch".into(), json!(b));
    }
    if let Some(pr) = &lane.pr {
        card.insert("pull request".into(), json!(clip(&format!("#{} {}", pr.number, pr.title))));
    }
    card.insert("status".into(), json!(status_words(s)));
    let doing = s.excerpt.as_deref().filter(|e| !e.trim().is_empty()).unwrap_or(&s.headline);
    if !doing.trim().is_empty() {
        card.insert("latest".into(), json!(clip(&doing.replace('`', ""))));
    }
    if let Some(at) = s.started_at {
        card.insert("started".into(), json!(crate::jev::ago_words(at, now)));
    }
    card.insert("last prompted".into(), json!(crate::jev::ago_words(s.last_prompt_at, now)));
    if let Some(r) = related.filter(|r| !r.lines.is_empty()) {
        let lines: Vec<String> = r.lines.iter().map(|(you, line)| format!("{}: {}", if *you { "the user" } else { "the agent" }, clip(&line.replace('`', "")))).collect();
        card.insert("lines of its conversation that share words with the message".into(), json!(lines));
    }
    Value::Object(card)
}

fn lane_card(l: &Lane, now: i64) -> Value {
    let mut card = Map::new();
    card.insert("worktree".into(), json!(lane_name(l)));
    card.insert("repo".into(), json!(l.repo));
    card.insert("project".into(), json!(l.project));
    if let Some(b) = &l.branch {
        card.insert("branch".into(), json!(b));
    }
    if let Some(pr) = &l.pr {
        card.insert("pull request".into(), json!(clip(&format!("#{} {}", pr.number, pr.title))));
    }
    let mut titles: Vec<&Session> = l.sessions.iter().filter(|s| s.visitor.is_none() && !s.archived && !s.helps_an_agent() && !s.headless && !s.title.trim().is_empty()).collect();
    titles.sort_by_key(|s| -s.last_prompt_at);
    let recent: Vec<String> = titles.iter().take(5).map(|s| clip(&s.title)).collect();
    if !recent.is_empty() {
        card.insert("recent sessions".into(), json!(recent));
    }
    card.insert("last active".into(), json!(crate::jev::ago_words(l.last_activity, now)));
    // New work starts only in reusable worktrees (src/route.ts), so Jev knows which are, and
    // which of those hold no work.
    card.insert(
        "reusable".into(),
        json!(match l.reuse {
            Some(Reuse::Free) | Some(Reuse::Landed) => "yes, and free: it holds no work now",
            Some(Reuse::Open) | Some(Reuse::Review) => "yes, with work under way",
            None => "no: it's kept for its own piece of work",
        }),
    );
    Value::Object(card)
}

/// The repos what's typed names, by `<project>/<repo>`: a word that is a repo's name, or a
/// project's (all its repos). Words split at anything but letters, digits, `-`, `_` and `.`, so
/// `spark-infra` doesn't name `spark`.
fn named_repos(text: &str, lanes: &[&Lane]) -> Vec<String> {
    let words: HashSet<String> = text
        .split(|c: char| !(c.is_alphanumeric() || matches!(c, '-' | '_' | '.')))
        .map(|w| w.trim_matches('.').to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    let names = |l: &Lane| words.contains(&l.repo.to_lowercase()) || (l.project != ELSEWHERE && words.contains(&l.project.to_lowercase()));
    let mut out: Vec<String> = lanes.iter().filter(|l| names(l)).map(|l| format!("{}/{}", l.project, l.repo)).collect();
    out.sort();
    out.dedup();
    out
}

/// The sessions and worktrees to offer, in the panel's project. Each session once, from the
/// lane it runs in; archived ones, long-stopped ones, helpers (whose agent talks to them, not
/// you, unless you've taken one up: `Helper::yours`) and headless runs (whose script does) stay
/// out. After them come up to `RELATED` sessions whose conversations share the message's words
/// (`related`, the likeliest first), however long ago they stopped: Jev can't pick a session it
/// isn't shown. Returned with those sessions' lines.
fn candidates<'a, 'r>(board: &'a Board, project: &str, related: &'r [crate::search::Related], now: i64) -> (Vec<(&'a Lane, &'a Session)>, Vec<&'a Lane>, Vec<&'r crate::search::Related>) {
    let lanes: Vec<&Lane> = board.lanes.iter().filter(|l| in_project(l, project)).collect();
    let offered: Vec<(&Lane, &Session)> = lanes
        .iter()
        .flat_map(|l| l.sessions.iter().map(move |s| (*l, s)))
        .filter(|(_, s)| s.visitor.is_none() && !s.archived && !s.helps_an_agent() && !s.headless)
        .collect();
    let mut sessions: Vec<(&Lane, &Session)> = offered.iter().copied().filter(|(_, s)| s.live || now - s.last_prompt_at < STOPPED_WITHIN_MS).collect();
    sessions.sort_by(|a, b| b.1.live.cmp(&a.1.live).then(b.1.last_prompt_at.cmp(&a.1.last_prompt_at)).then(a.1.key.cmp(&b.1.key)));
    sessions.truncate(MAX_SESSIONS);
    let related: Vec<&crate::search::Related> = related.iter().filter(|r| offered.iter().any(|(_, s)| s.key == r.key)).take(RELATED).collect();
    for r in &related {
        if !sessions.iter().any(|(_, s)| s.key == r.key) {
            sessions.extend(offered.iter().find(|(_, s)| s.key == r.key));
        }
    }
    let mut lanes = lanes;
    lanes.sort_by(|a, b| b.last_activity.cmp(&a.last_activity).then(a.id.cmp(&b.id)));
    lanes.truncate(MAX_LANES);
    (sessions, lanes, related)
}

/// The accounts a new session can start on: on, and with somewhere to start.
fn startable(cfg: &crate::config::Config) -> impl Iterator<Item = &crate::config::AccountCfg> {
    let home = crate::util::home();
    cfg.accounts.iter().filter(move |a| !a.disabled && crate::start::ordered_places(a, &home).is_ok_and(|p| !p.is_empty()))
}

/// The agent and model options offered to Jev when `[routing] prompt` is set, with an account
/// that has room for each.
fn agent_candidates(board: &Board, cfg: &crate::config::Config, now: i64) -> (Map<String, Value>, Vec<(String, Option<String>)>) {
    let mut cards = Map::new();
    let mut picks = Vec::new();
    if cfg.routing.prompt.trim().is_empty() {
        return (cards, picks);
    }
    let mut seen_harnesses = Vec::new();
    for a in startable(cfg) {
        let h = crate::model::Harness::from_config(&a.harness);
        if !seen_harnesses.contains(&h) {
            seen_harnesses.push(h);
        }
    }
    for h in seen_harnesses {
        let adapter = crate::harness::adapter(h);
        let word = h.word();
        if adapter.takes_model() {
            let Some(sample) = cfg.accounts.iter().find(|a| !a.disabled && crate::model::Harness::from_config(&a.harness) == h) else { continue };
            for m in adapter.models(sample) {
                let Some(acct) = crate::start::pick_account_where(cfg, &board.usage, &[], Some(&m.id), now, |a| crate::model::Harness::from_config(&a.harness) == h) else { continue };
                let key = format!("a{}", picks.len() + 1);
                cards.insert(key, json!({ "agent": word, "model": m.id, "description": m.label }));
                picks.push((acct, Some(m.id)));
            }
        } else {
            let Some(acct) = crate::start::pick_account_where(cfg, &board.usage, &[], None, now, |a| crate::model::Harness::from_config(&a.harness) == h) else { continue };
            let key = format!("a{}", picks.len() + 1);
            cards.insert(key, json!({ "agent": word }));
            picks.push((acct, None));
        }
    }
    for g in &cfg.routing.groups {
        let key = format!("a{}", picks.len() + 1);
        cards.insert(key, json!({ "group": g.name, "strategy": g.strategy.word() }));
        picks.push((g.name.clone(), None));
    }
    (cards, picks)
}

/// The services to ask about, given each account's reading (`readings`) and the accounts that
/// can start a session (`startable`, in the order their lists keep): each once whoever names it
/// (`mcp::key`), the first name read standing for it. None when every one of them reaches the
/// same services, since the answer would change nothing then.
fn services(readings: &[crate::mcp::Mcp], startable: &[&str]) -> Vec<Service> {
    let mut by_key: BTreeMap<String, Service> = BTreeMap::new();
    let mut order = Vec::new();
    for account in startable {
        let Some(reading) = readings.iter().find(|m| m.account == *account) else { continue };
        for server in &reading.servers {
            let key = crate::mcp::key(&server.name);
            let service = by_key.entry(key.clone()).or_insert_with(|| {
                order.push(key);
                Service { name: server.name.clone(), tools: server.tools.clone(), accounts: Vec::new(), model: None }
            });
            if service.tools.is_empty() {
                service.tools = server.tools.clone();
            }
            if !service.accounts.iter().any(|a| a == account) {
                service.accounts.push(account.to_string());
            }
        }
    }
    let differ = by_key.values().any(|s| s.accounts.len() < startable.len());
    if !differ {
        return Vec::new();
    }
    order.into_iter().filter_map(|k| by_key.remove(&k)).collect()
}

/// `services` for the accounts that can start a session, each service's accounts led by the one
/// Auto would pick of them, on the model it would pick: its routing group, the board's sessions
/// and history, and its model fallback, as a start on Auto has them (`start::resolve_group`).
fn service_candidates(board: &Board, cfg: &crate::config::Config, now: i64) -> Vec<Service> {
    let names: Vec<&str> = startable(cfg).map(|a| a.name.as_str()).collect();
    services(&board.mcp, &names)
        .into_iter()
        .map(|mut s| {
            if let Some(Ok(first)) = crate::start::resolve_group(cfg, Some(board), "auto", &[], None, None, false, now, |a| s.accounts.contains(&a.name)) {
                s.accounts.retain(|a| *a != first.account);
                s.accounts.insert(0, first.account);
                s.model = first.model;
            }
            s
        })
        .collect()
}

/// The request for what you typed, in the panel's project.
#[cfg(test)]
pub fn request(board: &Board, project: &str, text: &str, model: &str, now: i64) -> Result<Request> {
    request_with_routing(board, None, project, text, &[], model, now)
}

/// `request`, with the sessions whose conversations share its words (`related`), and an `agent`
/// question when `[routing] prompt` is set in `cfg`.
pub fn request_with_routing(board: &Board, cfg: Option<&crate::config::Config>, project: &str, text: &str, related: &[crate::search::Related], model: &str, now: i64) -> Result<Request> {
    let text = text.trim();
    if text.is_empty() {
        bail!("Nothing to route.");
    }
    let (sessions, lanes, related) = candidates(board, project, related, now);
    if lanes.is_empty() {
        bail!("No worktrees in this project to route to.");
    }

    let mut session_cards = Map::new();
    for (i, (lane, s)) in sessions.iter().enumerate() {
        session_cards.insert(format!("s{}", i + 1), session_card(lane, s, related.iter().copied().find(|r| r.key == s.key), now));
    }
    let mut lane_cards = Map::new();
    for (i, l) in lanes.iter().enumerate() {
        lane_cards.insert(format!("w{}", i + 1), lane_card(l, now));
    }
    lane_cards.insert(
        "none".into(),
        json!("None of these: the message needs no repository at all. It's a general question or task (how something works in general, a fact, a word, a snippet, a piece of writing) that names nothing in them."),
    );

    let mut duplicate_options = Map::new();
    for (i, (_, s)) in sessions.iter().enumerate() {
        if could_be_doing(s) {
            let k = format!("s{}", i + 1);
            duplicate_options.insert(k.clone(), session_cards[&k].clone());
        }
    }
    duplicate_options.insert("none".into(), json!("None of these sessions is doing it."));

    let mut session_options = session_cards.clone();
    session_options.insert(
        "none".into(),
        json!("None of these sessions: the message asks for new work, or names no session listed here."),
    );

    let mut images = crate::jev::Images::default();
    let message = images.swap(text);

    // A search is told apart by what it lacks: Jev took a session's own title, typed to find it,
    // for a message to that session until `find` said a search asks and says nothing, and that
    // words of a title are one (jev.md).
    let actions = json!({
        "send": "The message says something to one of the sessions in `sessions`: it tells it to do something, asks it a question, answers it, corrects it or follows up on its work.",
        "start": "The message asks for work, or asks a question, that no session in `sessions` is doing, so a new session should start on it.",
        "find": "The message asks for nothing and says nothing to anyone: it is only a few words to find a session by, such as a session's title or words from it, a branch, a pull request, a repo or a topic. Words that are, or are part of, a session's title are a search for that session.",
        "todo": "The message is a note to do something later rather than now, and says so (for example 'later', 'todo', 'remember to', 'at some point', 'next week').",
    });

    let (agent_cards, agents) = cfg.map(|c| agent_candidates(board, c, now)).unwrap_or_default();
    let services = cfg.map(|c| service_candidates(board, c, now)).unwrap_or_default();

    let mut body = json!({
        "model": model,
        "state": {
            "message": message,
            "sessions": Value::Object(session_cards),
        },
        "questions": {
            "action": {
                "type": "choice",
                "instructions": "`message` was typed by a developer into a launcher for their coding agents. What should happen with it?",
                "criteria": actions,
            },
            "session": {
                "type": "choice",
                "instructions": "Which coding agent session is `message` about or meant for?",
                "criteria": Value::Object(session_options),
            },
            "worktree": {
                "type": "choice",
                "instructions": "Which repository worktree does the work in `message` belong in? Match the project, product, repo, feature or files it talks about.",
                "criteria": Value::Object(lane_cards),
            },
            "duplicate": {
                "type": "choice",
                "instructions": "Which of these sessions is already doing the work `message` asks for?",
                "criteria": Value::Object(duplicate_options),
            },
            "own_worktree": {
                "type": "noul",
                "instructions": "Does `message` ask for its work to be kept apart on a new branch or in a new worktree, rather than done in one of the worktrees listed?",
                "criteria": {
                    "true": "It says so: a new branch, a new worktree, in parallel, separately, without touching the work already under way.",
                    "false": "It says nothing about branches or worktrees. Most features, fixes, refactors, questions and reviews are done in a listed worktree.",
                },
            },
        },
    });
    if !agent_cards.is_empty() {
        let prompt = cfg.map(|c| c.routing.prompt.trim()).unwrap_or_default();
        body["questions"]["agent"] = json!({
            "type": "choice",
            "instructions": format!("When starting a new session for `message`, which coding agent and model should run it? Follow the user's routing rules:\n{prompt}"),
            "criteria": Value::Object(agent_cards),
        });
    }

    if !services.is_empty() {
        body["questions"]["needs"] = needs_question(&services);
    }

    Ok(Request {
        body,
        sessions: sessions.iter().map(|(_, s)| s.key.clone()).collect(),
        running: sessions.iter().map(|(_, s)| s.live).collect(),
        lanes: lanes.iter().map(|l| l.id.clone()).collect(),
        lane_repos: lanes.iter().map(|l| format!("{}/{}", l.project, l.repo)).collect(),
        named: named_repos(text, &lanes),
        images,
        agents,
        services,
        related: related.iter().filter(|r| !r.lines.is_empty()).map(|r| r.key.clone()).collect(),
    })
}

/// Which service `message` needs: one option for each (`c1`, `c2`, …), its tools saying what it
/// does where its name doesn't.
fn needs_question(services: &[Service]) -> Value {
    let mut criteria = Map::new();
    for (i, s) in services.iter().enumerate() {
        let mut card = Map::new();
        card.insert("service".into(), json!(s.name));
        if !s.tools.is_empty() {
            let more = if s.tools.len() > SERVICE_TOOLS { ", …" } else { "" };
            card.insert("tools".into(), json!(format!("{}{more}", s.tools[..s.tools.len().min(SERVICE_TOOLS)].join(", "))));
        }
        criteria.insert(format!("c{}", i + 1), Value::Object(card));
    }
    criteria.insert(
        "none".into(),
        json!("None of these: the message needs no outside service, or one that isn't listed. Work on code, files, tests, branches, commits and pull requests needs none: the agents already have the repositories, git, GitHub's command line and a shell."),
    );
    json!({
        "type": "choice",
        "instructions": "The user's coding agents can reach these services through tools connected to them. Is what `message` asks about kept in one of them (mail, a calendar, chat messages, meeting notes, documents, designs, records), so that the agent must use that service to do it? Which one?",
        "criteria": Value::Object(criteria),
    })
}

/// How many runners-up the panel gets.
const TOP: usize = 3;

fn probabilities(answer: &Value) -> BTreeMap<String, f64> {
    answer
        .get("probabilities")
        .and_then(Value::as_object)
        .map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_f64()?))).collect())
        .unwrap_or_default()
}

/// Jev's session picks leaning toward running sessions: one that isn't running counts
/// `STOPPED_WEIGHT` as likely as Jev says, and the running ones share what it gives up, so the
/// sessions together stay as likely as Jev made them and "none" keeps its chance. With only
/// running or only stopped sessions picked, nothing changes.
fn favor_running(picks: Vec<(Pick, bool)>) -> Vec<Pick> {
    let total: f64 = picks.iter().map(|(p, _)| p.p).sum();
    let weighed: Vec<Pick> = picks.into_iter().map(|(p, running)| Pick { p: if running { p.p } else { p.p * STOPPED_WEIGHT }, ..p }).collect();
    let left: f64 = weighed.iter().map(|p| p.p).sum();
    if left <= 0.0 {
        return weighed;
    }
    let scale = total / left;
    weighed.into_iter().map(|p| Pick { p: p.p * scale, ..p }).collect()
}

fn ranked(mut picks: Vec<Pick>) -> Vec<Pick> {
    picks.sort_by(|a, b| b.p.partial_cmp(&a.p).unwrap_or(std::cmp::Ordering::Equal).then(a.id.cmp(&b.id)));
    picks
}

/// Reads Jev's answer into a route. The action is Jev's likeliest, with one correction made in
/// code: sending needs a session, so when "none" is the likelier session, new work is meant. A
/// search that fits none of the sessions stays a search: it's for something the panel finds by
/// its words (a PR, an older session), not work to start. Sessions are ranked with running ones
/// favored (`favor_running`), but for a search: opening a session resumes nothing, so it takes
/// Jev's own, and the panel can tell from them whether any fits (route.ts `suggestions`).
pub fn read(req: &Request, response: &Value, ms: u64) -> Result<Route> {
    let answers = response.get("answers").ok_or_else(|| anyhow!("TypeSafe's answer has no answers"))?;
    let action_p = probabilities(&answers["action"]);
    let session_p = probabilities(&answers["session"]);
    let lane_p = probabilities(&answers["worktree"]);

    let mut actions: Vec<(Action, f64)> = Action::ALL.iter().map(|a| (*a, action_p.get(a.key()).copied().unwrap_or(0.0))).collect();
    actions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let at = |id: &str, prefix: char, list: &[String]| -> Option<String> {
        let n: usize = id.strip_prefix(prefix)?.parse().ok()?;
        list.get(n.checked_sub(1)?).cloned()
    };
    let picked: Vec<(Pick, bool)> = session_p
        .iter()
        .filter_map(|(k, p)| {
            let n: usize = k.strip_prefix('s')?.parse().ok()?;
            let i = n.checked_sub(1)?;
            Some((Pick { id: req.sessions.get(i)?.clone(), p: *p }, req.running.get(i).copied().unwrap_or(false)))
        })
        .collect();
    // Whether any session fits at all is Jev's call, before running ones are favored.
    let top_session = picked.iter().map(|(p, _)| p.p).fold(0.0, f64::max);
    let no_session = session_p.get("none").copied().unwrap_or(if req.sessions.is_empty() { 1.0 } else { 0.0 });
    let mut action = actions.first().map_or(Action::Start, |a| a.0);
    if action == Action::Send && no_session > top_session {
        action = Action::Start;
    }
    let sessions = ranked(if action == Action::Find { picked.into_iter().map(|(p, _)| p).collect() } else { favor_running(picked) });
    let mut repo_sum: BTreeMap<String, f64> = BTreeMap::new();
    for (k, p) in &lane_p {
        if let Some(repo) = at(k, 'w', &req.lane_repos) {
            *repo_sum.entry(repo).or_default() += p;
        }
    }
    let repos = ranked(repo_sum.into_iter().map(|(id, p)| Pick { id, p }).collect());
    // A repo named in what's typed, when it's Jev's likeliest, keeps the others out: "in
    // spark" leaves no row for a worktree of another repo Jev gave a little to. When Jev
    // leans elsewhere, the word may not mean the repo, and nothing is left out.
    let named = |repo: &str| req.named.iter().any(|n| n == repo);
    let only_named = repos.first().is_some_and(|top| named(&top.id));
    let repos: Vec<Pick> = repos.into_iter().filter(|r| !only_named || named(&r.id)).collect();
    let lanes = ranked(
        lane_p
            .iter()
            .filter(|(k, _)| !only_named || at(k, 'w', &req.lane_repos).is_some_and(|r| named(&r)))
            .filter_map(|(k, p)| Some(Pick { id: at(k, 'w', &req.lanes)?, p: *p }))
            .collect(),
    );

    let duplicate = ranked(probabilities(&answers["duplicate"]).iter().filter_map(|(k, p)| Some(Pick { id: at(k, 's', &req.sessions)?, p: *p })).collect())
        .into_iter()
        .next()
        .filter(|d| d.p >= DUPLICATE_AT);

    let mut agents: Vec<AgentPick> = probabilities(&answers["agent"])
        .iter()
        .filter_map(|(k, p)| {
            let n: usize = k.strip_prefix('a')?.parse().ok()?;
            let (account, model) = req.agents.get(n.checked_sub(1)?)?.clone();
            (*p >= SCORED_AT).then_some(AgentPick { account, model, p: *p })
        })
        .collect();
    agents.sort_by(|a, b| b.p.partial_cmp(&a.p).unwrap_or(std::cmp::Ordering::Equal).then(a.account.cmp(&b.account)));
    agents.truncate(TOP);

    let needs_p = probabilities(&answers["needs"]);
    let no_need = needs_p.get("none").copied().unwrap_or(0.0);
    let needs = needs_p
        .iter()
        .filter_map(|(k, p)| Some((req.services.get(k.strip_prefix('c')?.parse::<usize>().ok()?.checked_sub(1)?)?, *p)))
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .filter(|(_, p)| *p >= NEEDS_AT && *p > no_need)
        .map(|(s, p)| NeedPick { name: s.name.clone(), accounts: s.accounts.clone(), model: s.model.clone(), p });

    Ok(Route {
        action,
        actions,
        sessions: sessions.into_iter().filter(|s| s.p >= SCORED_AT).take(SCORED).collect(),
        no_session,
        lanes: lanes.into_iter().take(TOP).collect(),
        no_repo: lane_p.get("none").copied().unwrap_or(0.0),
        repos: repos.into_iter().take(TOP).collect(),
        own_worktree: answers["own_worktree"]["noul"].as_f64().unwrap_or(0.0),
        own_worktree_asked: answers["own_worktree"]["noul"].as_f64().unwrap_or(0.0),
        habit: Default::default(),
        duplicate,
        agents,
        needs,
        model: response["model"].as_str().unwrap_or_default().to_string(),
        ms,
        input_tokens: response["usage"]["input_tokens"].as_u64().unwrap_or(0),
    })
}

/// Where typed text goes on this device's board, for its quick panel or a linked phone's:
/// `route`, with the settings as they stand, and only while `[jev] quick_panel` is on.
pub fn suggest(board: &Board, project: &str, text: &str) -> Result<Route> {
    let cfg = crate::config::current()?;
    if !cfg.jev.quick_panel {
        bail!("Suggestions in the quick panel are off.");
    }
    route(board, &cfg, project, text)
}

/// Routes what you typed: builds the request, asks Jev, reads the answer.
pub fn route(board: &Board, cfg: &crate::config::Config, project: &str, text: &str) -> Result<Route> {
    let searched = Instant::now();
    let related = crate::search::related(text, RELATED_LOOKED);
    if crate::perf::enabled() {
        crate::perf::log(&format!("route: {} sessions share words, in {} ms", related.len(), searched.elapsed().as_millis()));
    }
    let mut req = request_with_routing(board, Some(cfg), project, text, &related, cfg.jev.model.trim(), crate::util::now_ms())?;
    let started = Instant::now();
    req.images.attach(&mut req.body);
    let response = crate::jev::ask(crate::jev::Feature::QuickPanel, &req.body)?;
    let route = read(&req, &response, started.elapsed().as_millis() as u64)?;
    crate::routelog::asked(project, text, crate::routelog::Asked { model: route.model.clone(), ms: route.ms, related: req.related.clone() });
    let habit = route.repos.first().map(|r| crate::starts::habit(&r.id)).unwrap_or_default();
    Ok(with_habit(route, habit))
}

/// Weighs Jev's guess at a new worktree by how you've started work in the likeliest repo. A new
/// worktree is suggested only in a repo where you often start in one (`starts::frequent`):
/// prompts read as features and fixes, which Jev takes for work of its own, and elsewhere new
/// work goes to a reusable worktree.
fn with_habit(mut route: Route, habit: crate::starts::Habit) -> Route {
    route.own_worktree = if crate::starts::frequent(habit) { crate::starts::weigh(route.own_worktree_asked, habit) } else { 0.0 };
    route.habit = habit;
    route
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(key: &str, title: &str, status: Status, live: bool, at: i64) -> Session {
        Session { status, live, title: title.into(), last_activity: at, last_prompt_at: at, excerpt: Some("Ran `cargo test`".into()), branch: Some("feature".into()), ..crate::fixtures::session(key) }
    }

    fn lane(id: &str, project: &str, repo: &str, sessions: Vec<Session>, at: i64) -> Lane {
        Lane { project: project.into(), repo: repo.into(), callsign: repo.into(), branch: Some("main".into()), last_activity: at, ..crate::fixtures::lane(id, sessions) }
    }

    const NOW: i64 = 10 * 24 * 3600 * 1000;

    fn board() -> Board {
        let old = NOW - 5 * 24 * 3600 * 1000;
        let mut archived = session("c:a", "Archived one", Status::Idle, false, NOW);
        archived.archived = true;
        let mut helper = session("c:h", "Review the diff", Status::Working, true, NOW);
        helper.helper = Some(crate::model::Helper { parent: Some("c:1".into()), pr: None, report: false, yours: false });
        let mut run = session("c:p", "cwd-29", Status::Working, true, NOW);
        run.headless = true;
        let mut elsewhere = lane("/tmp/x", ELSEWHERE, "x", vec![session("c:x", "Scratch", Status::Idle, true, NOW)], NOW);
        elsewhere.hidden = true;
        Board {
            lanes: vec![
                lane("/p/api", "Acme", "api", vec![session("c:1", "Fix login", Status::Working, true, NOW - 60_000), session("c:old", "Old stopped", Status::Stopped, false, old), archived, helper, run], NOW - 60_000),
                lane("/p/web", "Acme", "web", vec![session("c:2", "Landing page copy", Status::Unread, true, NOW - 3_600_000)], NOW),
                lane("/p/blog", "Blog", "blog", vec![], NOW - 7_200_000),
                elsewhere,
            ],
            ..Default::default()
        }
    }

    #[test]
    fn offers_the_projects_live_and_recent_sessions_and_its_worktrees() {
        let b = board();
        let req = request(&b, "Acme", "  make the login button blue ", "jev-latest", NOW).unwrap();
        // Live first, then by last prompt; archived and long-stopped ones, helpers and headless
        // runs stay out.
        assert_eq!(req.sessions, vec!["c:1", "c:2"]);
        // The latest worktree first.
        assert_eq!(req.lanes, vec!["/p/web", "/p/api"]);
        assert_eq!(req.body["state"]["message"], "make the login button blue");
        assert_eq!(req.body["state"]["sessions"]["s1"]["title"], "Fix login");
        assert_eq!(req.body["state"]["sessions"]["s1"]["status"], "working on a turn right now");
        assert_eq!(req.body["state"]["sessions"]["s1"]["latest"], "Ran cargo test");
        assert_eq!(req.body["state"]["sessions"]["s1"]["last prompted"], "just now");
        assert!(req.body["questions"]["session"]["criteria"]["none"].is_string());
        assert!(req.body["questions"]["worktree"]["criteria"]["none"].is_string(), "a question can need no repo at all");
        assert_eq!(req.body["questions"]["worktree"]["criteria"]["w2"]["worktree"], "api (main checkout of api)");
        assert_eq!(req.body["questions"]["worktree"]["criteria"]["w2"]["recent sessions"], json!(["Fix login", "Old stopped"]));
        assert_eq!(req.body["questions"]["worktree"]["criteria"]["w2"]["reusable"], "no: it's kept for its own piece of work");
        assert_eq!(req.body["model"], "jev-latest");
        // Only running sessions that aren't waiting on you could already be doing it.
        let dup = req.body["questions"]["duplicate"]["criteria"].as_object().unwrap();
        assert_eq!(dup.keys().collect::<Vec<_>>(), vec!["none", "s1", "s2"]);
    }

    #[test]
    fn sessions_whose_conversations_share_the_words_come_with_their_lines() {
        let b = board();
        let related = |key: &str, line: &str| crate::search::Related { key: key.into(), shared: vec!["backoff".into()], lines: vec![(true, line.into()), (false, "Added `backoff` to the retries".into())] };
        let found = [related("c:old", "Retry the webhook with backoff"), related("c:2", "the backoff copy"), related("c:a", "archived"), related("claude:elsewhere", "not on the board")];
        let req = request_with_routing(&b, None, "Acme", "add jitter to the backoff", &found, "m", NOW).unwrap();
        // Stopped five days ago, too long to be offered otherwise, it comes after the others;
        // archived ones and ones not in the project stay out.
        assert_eq!(req.sessions, vec!["c:1", "c:2", "c:old"]);
        assert_eq!(req.related, vec!["c:old", "c:2"]);
        let lines = &req.body["state"]["sessions"]["s3"]["lines of its conversation that share words with the message"];
        assert_eq!(lines, &json!(["the user: Retry the webhook with backoff", "the agent: Added backoff to the retries"]));
        assert!(req.body["state"]["sessions"]["s1"].get("lines of its conversation that share words with the message").is_none());
    }

    #[test]
    fn a_pasted_image_goes_where_it_was_in_the_message() {
        let req = request(&board(), "Acme", "make it look like ![image](images/0123456789abcdef.png) please", "jev-1.14.0", NOW).unwrap();
        assert_eq!(req.body["state"]["message"], "make it look like image-0123456789abcdef please");
    }

    #[test]
    fn reads_a_duplicate_only_when_jev_is_fairly_sure() {
        let b = board();
        let req = request(&b, "Acme", "fix the login bug", "m", NOW).unwrap();
        let mut v = answer(&[("start", 1.0)], &[("s1", 0.5), ("s2", 0.005), ("none", 0.495)], &[("w1", 1.0)], 0.0);
        v["answers"]["duplicate"]["probabilities"] = json!({ "s1": 0.7, "none": 0.3 });
        let r = read(&req, &v, 1).unwrap();
        assert_eq!(r.duplicate, Some(Pick { id: "c:1".into(), p: 0.7 }));
        // Sessions Jev gives next to no chance aren't ranked.
        assert_eq!(r.sessions.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), vec!["c:1"]);
        v["answers"]["duplicate"]["probabilities"] = json!({ "s1": 0.45, "none": 0.55 });
        assert_eq!(read(&req, &v, 1).unwrap().duplicate, None);
    }

    #[test]
    fn all_projects_leaves_hidden_worktrees_out_and_elsewhere_has_them() {
        let b = board();
        assert_eq!(request(&b, ALL_PROJECTS, "hi", "m", NOW).unwrap().lanes.len(), 3);
        assert_eq!(request(&b, ELSEWHERE, "hi", "m", NOW).unwrap().lanes, vec!["/tmp/x"]);
        assert!(request(&b, "Nope", "hi", "m", NOW).is_err());
        assert!(request(&b, "Acme", "  ", "m", NOW).is_err());
    }

    fn answer(action: &[(&str, f64)], session: &[(&str, f64)], worktree: &[(&str, f64)], own: f64) -> Value {
        let map = |xs: &[(&str, f64)]| Value::Object(xs.iter().map(|(k, p)| (k.to_string(), json!(p))).collect());
        json!({
            "model": "jev-1.13.0",
            "answers": {
                "action": { "type": "choice", "probabilities": map(action) },
                "session": { "type": "choice", "probabilities": map(session) },
                "worktree": { "type": "choice", "probabilities": map(worktree) },
                "own_worktree": { "type": "noul", "noul": own },
                "duplicate": { "type": "choice", "probabilities": map(&[("none", 1.0)]) },
            },
            "usage": { "input_tokens": 1234, "output_tokens": 50 },
        })
    }

    #[test]
    fn reads_the_answer_back_into_keys_and_ids() {
        let b = board();
        let req = request(&b, "Acme", "the login fix should also cover sso", "m", NOW).unwrap();
        let r = read(&req, &answer(&[("send", 0.8), ("start", 0.15), ("find", 0.05)], &[("s1", 0.9), ("s2", 0.02), ("none", 0.08)], &[("w1", 0.3), ("w2", 0.7)], 0.1), 250).unwrap();
        assert_eq!(r.action, Action::Send);
        assert_eq!(r.actions[0], (Action::Send, 0.8));
        assert_eq!(r.sessions[0], Pick { id: "c:1".into(), p: 0.9 });
        assert_eq!(r.no_session, 0.08);
        assert_eq!(r.lanes[0].id, "/p/api");
        assert_eq!(r.repos[0], Pick { id: "Acme/api".into(), p: 0.7 });
        assert_eq!(r.no_repo, 0.0);
        assert_eq!((r.model.as_str(), r.ms, r.input_tokens), ("jev-1.13.0", 250, 1234));
    }

    #[test]
    fn reads_work_that_needs_no_repo() {
        let b = board();
        let req = request(&b, "Acme", "what's the difference between a mutex and a semaphore", "m", NOW).unwrap();
        let r = read(&req, &answer(&[("start", 0.9), ("send", 0.1)], &[("none", 1.0)], &[("w1", 0.1), ("w2", 0.2), ("none", 0.7)], 0.0), 1).unwrap();
        assert_eq!(r.no_repo, 0.7);
        // The worktrees keep what Jev gave them, so a runner-up still has its row.
        assert_eq!(r.lanes.iter().map(|p| (p.id.as_str(), p.p)).collect::<Vec<_>>(), vec![("/p/api", 0.2), ("/p/web", 0.1)]);
    }

    #[test]
    fn a_running_session_leads_a_stopped_one_jev_scored_a_little_higher() {
        let mut b = board();
        b.lanes[0].sessions[1].last_prompt_at = NOW - 3_600_000;
        let req = request(&b, "Acme", "the login fix should also cover sso", "m", NOW).unwrap();
        // c:1 and c:2 run; c:old doesn't.
        assert_eq!(req.sessions, vec!["c:1", "c:2", "c:old"]);
        let sends = |s1: f64, s3: f64| read(&req, &answer(&[("send", 1.0)], &[("s1", s1), ("s3", s3), ("none", 0.1)], &[("w1", 1.0)], 0.0), 1).unwrap();
        let r = sends(0.35, 0.55);
        assert_eq!(r.action, Action::Send);
        assert_eq!(r.sessions.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), vec!["c:1", "c:old"]);
        // Together they're as likely as Jev said, and none keeps its share.
        assert!((r.sessions.iter().map(|p| p.p).sum::<f64>() - 0.9).abs() < 1e-9);
        assert_eq!(r.no_session, 0.1);
        // One Jev is much surer of still leads.
        assert_eq!(sends(0.1, 0.8).sessions[0].id, "c:old");
        // With no running session picked, Jev's numbers stand.
        assert_eq!(sends(0.0, 0.9).sessions[0], Pick { id: "c:old".into(), p: 0.9 });
    }

    #[test]
    fn sending_without_a_likely_session_is_new_work() {
        let b = board();
        let req = request(&b, "Acme", "add a pricing page", "m", NOW).unwrap();
        let r = read(&req, &answer(&[("send", 0.55), ("start", 0.45)], &[("s1", 0.2), ("none", 0.7)], &[("w1", 1.0)], 0.9), 1).unwrap();
        assert_eq!(r.action, Action::Start);
        assert_eq!(r.own_worktree, 0.9);
    }

    #[test]
    fn a_search_that_fits_no_session_stays_a_search() {
        let b = board();
        let req = request(&b, "Acme", "PR 418", "m", NOW).unwrap();
        let r = read(&req, &answer(&[("find", 0.9), ("start", 0.1)], &[("s1", 0.1), ("none", 0.9)], &[("w1", 1.0)], 0.0), 1).unwrap();
        assert_eq!(r.action, Action::Find);
        assert_eq!(r.no_session, 0.9);
    }

    #[test]
    fn a_search_takes_jevs_sessions_as_they_are() {
        let mut b = board();
        b.lanes[0].sessions[1].last_prompt_at = NOW - 3_600_000;
        let req = request(&b, "Acme", "old stopped", "m", NOW).unwrap();
        // c:1 runs; c:old doesn't, and favoring c:1 would leave c:old less likely than none.
        let answered = |action: &str| read(&req, &answer(&[(action, 1.0)], &[("s1", 0.2), ("s3", 0.45), ("none", 0.35)], &[("w1", 1.0)], 0.0), 1).unwrap();
        let found = answered("find");
        assert_eq!(found.sessions, vec![Pick { id: "c:old".into(), p: 0.45 }, Pick { id: "c:1".into(), p: 0.2 }]);
        assert!(found.sessions[0].p > found.no_session);
        let sent = answered("send");
        assert_eq!(sent.action, Action::Send);
        assert!(sent.sessions[0].p < sent.no_session);
    }

    #[test]
    fn a_habit_of_existing_worktrees_outweighs_jevs_yes() {
        let b = board();
        let req = request(&b, "Acme", "add a settings page", "jev-latest", NOW).unwrap();
        let r = read(&req, &answer(&[("start", 1.0)], &[("none", 1.0)], &[("w1", 1.0)], 0.9), 1).unwrap();
        let r = with_habit(r, crate::starts::Habit { new: 1, of: 20 });
        assert_eq!(r.own_worktree_asked, 0.9);
        assert!(r.own_worktree < 0.5);
    }

    #[test]
    fn a_new_worktree_only_where_you_often_start_in_one() {
        let b = board();
        let req = request(&b, "Acme", "add a settings page", "jev-latest", NOW).unwrap();
        let r = read(&req, &answer(&[("start", 1.0)], &[("none", 1.0)], &[("w1", 1.0)], 0.9), 1).unwrap();
        // No habit yet: Jev's yes isn't enough.
        assert_eq!(with_habit(r.clone(), crate::starts::Habit::default()).own_worktree, 0.0);
        assert!(with_habit(r, crate::starts::Habit { new: 4, of: 6 }).own_worktree > 0.8);
    }

    #[test]
    fn a_repo_named_in_the_message_keeps_the_others_out_when_jev_agrees() {
        let b = board();
        // "web" is a repo; "web-app" or "webby" wouldn't be.
        let req = request(&b, "Acme", "Add a pricing section to web.", "m", NOW).unwrap();
        assert_eq!(req.named, vec!["Acme/web"]);
        let web = req.lanes.iter().position(|l| l == "/p/web").unwrap() + 1;
        let api = req.lanes.iter().position(|l| l == "/p/api").unwrap() + 1;
        let (w, a) = (format!("w{web}"), format!("w{api}"));
        let r = read(&req, &answer(&[("start", 1.0)], &[("none", 1.0)], &[(&w, 0.7), (&a, 0.3)], 0.0), 1).unwrap();
        assert_eq!(r.lanes.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), vec!["/p/web"]);
        assert_eq!(r.repos.len(), 1);
        // When Jev leans to another repo, the word may mean something else: nothing is left out.
        let r = read(&req, &answer(&[("start", 1.0)], &[("none", 1.0)], &[(&w, 0.3), (&a, 0.7)], 0.0), 1).unwrap();
        assert_eq!(r.lanes.len(), 2);
        assert!(request(&b, "Acme", "tidy the web-app and webby", "m", NOW).unwrap().named.is_empty());
    }

    fn reading(account: &str, servers: &[(&str, &[&str])]) -> crate::mcp::Mcp {
        let servers = servers.iter().map(|(name, tools)| crate::mcp::Server { tools: tools.iter().map(|t| t.to_string()).collect(), ..crate::mcp::Server::new(*name, "claude.ai connector") }).collect();
        crate::mcp::Mcp { account: account.into(), servers, note: None, as_of: Some(1), stale: None }
    }

    #[test]
    fn services_are_asked_about_only_when_the_accounts_differ() {
        let readings = [
            reading("claude", &[("Gmail", &["search_threads"]), ("Slack", &[])]),
            reading("claude2", &[("Slack", &["slack_read_channel"])]),
            reading("codex", &[("gmail", &[]), ("Slack", &[])]),
            reading("off", &[("Kestrel", &[])]),
        ];
        let got = services(&readings, &["claude", "claude2", "codex"]);
        let names = |s: &[Service]| s.iter().map(|s| (s.name.clone(), s.accounts.join(" "))).collect::<Vec<_>>();
        // Gmail and gmail are one; an account that can't start (off) isn't asked about.
        assert_eq!(names(&got), [("Gmail".to_string(), "claude codex".to_string()), ("Slack".to_string(), "claude claude2 codex".to_string())]);
        assert_eq!(got[1].tools, ["slack_read_channel"], "the tools of whichever account lists them");
        // Every account reaching the same services: nothing to ask.
        assert!(services(&readings, &["claude", "codex"]).is_empty());
        assert!(services(&readings, &["claude2"]).is_empty());
        assert!(services(&[], &["claude", "codex"]).is_empty());
    }

    #[test]
    fn a_service_leads_with_the_account_and_model_auto_would_start() {
        use crate::model::{Meter, Severity};
        let (one, two) = (crate::fixtures::scratch("needs-one"), crate::fixtures::scratch("needs-two"));
        std::fs::write(one.join("settings.json"), r#"{"model": "claude-fable-5-1"}"#).unwrap();
        let account = |name: &str, dir: &std::path::Path| crate::config::AccountCfg { name: name.into(), harness: "claude".into(), home: dir.to_string_lossy().into_owned(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let cfg = crate::config::Config { accounts: vec![account("claude", &one), account("claude2", &two)], ..Default::default() };
        let meter = |key: &str, percent: f64, model: Option<&str>| Meter { key: key.into(), label: key.into(), window: key.into(), percent, resets_at: None, severity: if percent >= 90.0 { Severity::Critical } else { Severity::Normal }, model: model.map(String::from) };
        let usage = |a: &crate::config::AccountCfg, meters: Vec<Meter>| crate::model::Usage { meters, as_of: Some(NOW), loading: false, ..crate::usage::loading(a) };
        let mut b = board();
        b.mcp = vec![reading("claude", &[("Gmail", &[]), ("Slack", &[])]), reading("claude2", &[("Slack", &[])])];
        // claude's week has room but its Fable limit is nearly used, so Auto starts it on another
        // model; claude2's week is nearly used.
        b.usage = vec![usage(&cfg.accounts[0], vec![meter("week", 10.0, None), meter("model:Fable", 95.0, Some("Fable"))]), usage(&cfg.accounts[1], vec![meter("week", 97.0, None)])];
        let got = service_candidates(&b, &cfg, NOW);
        let lead = |name: &str| got.iter().find(|s| s.name == name).map(|s| (s.accounts.clone(), s.model.clone())).unwrap();
        assert_eq!(lead("Gmail"), (vec!["claude".to_string()], Some("opus".to_string())));
        assert_eq!(lead("Slack"), (vec!["claude".to_string(), "claude2".to_string()], Some("opus".to_string())));
        // With claude's week nearly used instead, claude2 leads, on its own default.
        b.usage = vec![usage(&cfg.accounts[0], vec![meter("week", 97.0, None)]), usage(&cfg.accounts[1], vec![meter("week", 10.0, None)])];
        assert_eq!(service_candidates(&b, &cfg, NOW).iter().find(|s| s.name == "Slack").map(|s| (s.accounts[0].as_str(), s.model.clone())), Some(("claude2", None)));
    }

    #[test]
    fn the_needs_question_says_what_each_service_does() {
        let tools: Vec<String> = (1..=14).map(|i| format!("t{i}")).collect();
        let q = needs_question(&[Service { name: "Gmail".into(), tools, accounts: vec!["claude".into()], model: None }, Service { name: "Kestrel".into(), tools: vec![], accounts: vec!["codex".into()], model: None }]);
        assert_eq!(q["type"], "choice");
        assert_eq!(q["criteria"]["c1"]["service"], "Gmail");
        assert_eq!(q["criteria"]["c1"]["tools"], "t1, t2, t3, t4, t5, t6, t7, t8, t9, t10, t11, t12, …");
        assert!(q["criteria"]["c2"].get("tools").is_none());
        assert!(q["criteria"]["none"].is_string());
    }

    #[test]
    fn reads_the_service_the_work_needs_when_jev_is_sure_enough() {
        let b = board();
        let mut req = request(&b, "Acme", "what does my email say about the launch", "m", NOW).unwrap();
        assert!(req.body["questions"].get("needs").is_none(), "no settings, nothing to ask");
        req.services = vec![Service { name: "Gmail".into(), tools: vec![], accounts: vec!["codex".into(), "claude".into()], model: Some("gpt-6".into()) }, Service { name: "Slack".into(), tools: vec![], accounts: vec!["claude".into()], model: None }];
        let with_needs = |needs: Value| {
            let mut v = answer(&[("start", 1.0)], &[("none", 1.0)], &[("none", 1.0)], 0.0);
            v["answers"]["needs"] = json!({ "type": "choice", "probabilities": needs });
            read(&req, &v, 1).unwrap().needs
        };
        assert_eq!(with_needs(json!({ "c1": 0.8, "c2": 0.05, "none": 0.15 })), Some(NeedPick { name: "Gmail".into(), accounts: vec!["codex".into(), "claude".into()], model: Some("gpt-6".into()), p: 0.8 }));
        assert_eq!(with_needs(json!({ "c1": 0.45, "none": 0.55 })), None, "not sure enough");
        assert_eq!(with_needs(json!({ "c9": 0.9, "none": 0.1 })), None, "no such option");
        assert_eq!(read(&req, &answer(&[("start", 1.0)], &[("none", 1.0)], &[("w1", 1.0)], 0.0), 1).unwrap().needs, None, "not asked");
    }

    #[test]
    fn repos_add_up_their_worktrees() {
        let mut b = board();
        let mut wt = lane("/p/api-wt", "Acme", "api", vec![], NOW - 1);
        wt.origin = Origin::Script;
        b.lanes.push(wt);
        let req = request(&b, "Acme", "x", "m", NOW).unwrap();
        let (api, api_wt) = (req.lanes.iter().position(|l| l == "/p/api").unwrap() + 1, req.lanes.iter().position(|l| l == "/p/api-wt").unwrap() + 1);
        let web = req.lanes.iter().position(|l| l == "/p/web").unwrap() + 1;
        let (a, w, x) = (format!("w{api}"), format!("w{web}"), format!("w{api_wt}"));
        let r = read(&req, &answer(&[("start", 1.0)], &[("none", 1.0)], &[(&w, 0.4), (&a, 0.35), (&x, 0.25)], 0.0), 1).unwrap();
        // One worktree of web beats either of api's, but api is the likelier repo.
        assert_eq!(r.lanes[0].id, "/p/web");
        assert_eq!(r.repos[0].id, "Acme/api");
        assert!((r.repos[0].p - 0.6).abs() < 1e-9);
    }
}
