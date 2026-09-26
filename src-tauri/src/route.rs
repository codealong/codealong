//! Routing for the quick panel: what you type goes to TypeSafe's Jev, which answers a few typed
//! questions about it at once (docs.typesafe.ai): is it for a session that's already going, new
//! work, a search, or a note for later; which session; which worktree; and does it want a
//! worktree of its own. Jev picks from lists and gives a probability for every option, so the
//! code here decides what to suggest and how sure to sound, and the panel only acts on ⏎.
//!
//! Jev reads questions literally and is weak at dates and numbers, so times go in as words
//! ("5 minutes ago") and each question asks one plain thing.
//!
//! The request goes through jev.rs, which holds the key.

use std::collections::BTreeMap;
use std::time::Instant;

use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::board::ELSEWHERE;
use crate::model::{Board, Lane, Origin, Session, Status};

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

/// What Jev thinks you want done with what you typed.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    /// Send it to a session that's already going.
    Send,
    /// Start a new session with it.
    Start,
    /// It names a session to open.
    Find,
    /// Keep it as a todo.
    Todo,
}

impl Action {
    fn key(self) -> &'static str {
        match self {
            Action::Send => "send",
            Action::Start => "start",
            Action::Find => "find",
            Action::Todo => "todo",
        }
    }
    const ALL: [Action; 4] = [Action::Send, Action::Start, Action::Find, Action::Todo];
}

/// One option Jev weighed, with its probability.
#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Pick {
    pub id: String,
    pub p: f64,
}

/// The suggestion, with the runners-up, for the panel to show.
#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub action: Action,
    /// How likely each action is, most likely first.
    pub actions: Vec<(Action, f64)>,
    /// Session keys, most likely first; those Jev gives any real chance, for ranking the search.
    pub sessions: Vec<Pick>,
    /// How likely it is that no listed session is meant.
    pub no_session: f64,
    /// Worktrees (lane ids), most likely first; the top ones only.
    pub lanes: Vec<Pick>,
    /// Repos, the worktrees' probabilities added up: which repo is clearer than which worktree.
    pub repos: Vec<Pick>,
    /// How likely it is that new work wants a worktree of its own.
    pub own_worktree: f64,
    /// A live session that may already be doing this work, when Jev thinks one is.
    pub duplicate: Option<Pick>,
    /// The versioned model that answered, such as `jev-1.13.0`.
    pub model: String,
    pub ms: u64,
    pub input_tokens: u64,
}

/// What goes to Jev, and how to read its answer back: option keys (`s1`, `w1`) stand for
/// session keys and lane ids, so long paths never reach the model as labels.
#[derive(Debug)]
pub struct Request {
    pub body: Value,
    sessions: Vec<String>,
    lanes: Vec<String>,
    lane_repos: Vec<String>,
}

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

/// Whether a lane is in the panel's project, as the panel decides it (quick.ts `inProject`).
fn in_project(lane: &Lane, project: &str) -> bool {
    if project == ALL_PROJECTS {
        return !lane.hidden;
    }
    lane.project == project && (project == ELSEWHERE || !lane.hidden)
}

fn status_words(s: &Session) -> &'static str {
    match s.status {
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

fn session_card(lane: &Lane, s: &Session, now: i64) -> Value {
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
    let mut titles: Vec<&Session> = l.sessions.iter().filter(|s| s.visitor.is_none() && !s.archived && !s.title.trim().is_empty()).collect();
    titles.sort_by_key(|s| -s.last_prompt_at);
    let recent: Vec<String> = titles.iter().take(5).map(|s| clip(&s.title)).collect();
    if !recent.is_empty() {
        card.insert("recent sessions".into(), json!(recent));
    }
    card.insert("last active".into(), json!(crate::jev::ago_words(l.last_activity, now)));
    Value::Object(card)
}

/// The sessions and worktrees to offer, in the panel's project. Each session once, from the
/// lane it runs in; archived ones and long-stopped ones stay out.
fn candidates<'a>(board: &'a Board, project: &str, now: i64) -> (Vec<(&'a Lane, &'a Session)>, Vec<&'a Lane>) {
    let lanes: Vec<&Lane> = board.lanes.iter().filter(|l| in_project(l, project)).collect();
    let mut sessions: Vec<(&Lane, &Session)> = lanes
        .iter()
        .flat_map(|l| l.sessions.iter().map(move |s| (*l, s)))
        .filter(|(_, s)| s.visitor.is_none() && !s.archived && (s.live || now - s.last_prompt_at < STOPPED_WITHIN_MS))
        .collect();
    sessions.sort_by(|a, b| b.1.live.cmp(&a.1.live).then(b.1.last_prompt_at.cmp(&a.1.last_prompt_at)).then(a.1.key.cmp(&b.1.key)));
    sessions.truncate(MAX_SESSIONS);
    let mut lanes = lanes;
    lanes.sort_by(|a, b| b.last_activity.cmp(&a.last_activity).then(a.id.cmp(&b.id)));
    lanes.truncate(MAX_LANES);
    (sessions, lanes)
}

/// The request for what you typed, in the panel's project.
pub fn request(board: &Board, project: &str, text: &str, model: &str, now: i64) -> Result<Request> {
    let text = text.trim();
    if text.is_empty() {
        bail!("Nothing to route.");
    }
    let (sessions, lanes) = candidates(board, project, now);
    if lanes.is_empty() {
        bail!("No worktrees in this project to route to.");
    }

    let mut session_cards = Map::new();
    for (i, (lane, s)) in sessions.iter().enumerate() {
        session_cards.insert(format!("s{}", i + 1), session_card(lane, s, now));
    }
    let mut lane_cards = Map::new();
    for (i, l) in lanes.iter().enumerate() {
        lane_cards.insert(format!("w{}", i + 1), lane_card(l, now));
    }

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

    let actions = json!({
        "send": "The message is for one of the sessions in `sessions`: it continues, answers, corrects, redirects or follows up on that session's work, or asks that session a question.",
        "start": "The message asks for work that no session in `sessions` is doing, so a new session should start on it.",
        "find": "The message only names or searches for a session in `sessions` (a few words of its title, a branch, a pull request) in order to open it. It asks for no work.",
        "todo": "The message is a note to do something later rather than now, and says so (for example 'later', 'todo', 'remember to', 'at some point', 'next week').",
    });

    let body = json!({
        "model": model,
        "state": {
            "message": text,
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
                "instructions": "Does `message` ask for a self-contained code change, such as a feature, a fix or a refactor, that should get a new branch of its own?",
                "criteria": {
                    "true": "A change that would be committed on its own branch.",
                    "false": "A question, an investigation, a review, a small tweak to work already under way, or no code change.",
                },
            },
        },
    });

    Ok(Request {
        body,
        sessions: sessions.iter().map(|(_, s)| s.key.clone()).collect(),
        lanes: lanes.iter().map(|l| l.id.clone()).collect(),
        lane_repos: lanes.iter().map(|l| format!("{}/{}", l.project, l.repo)).collect(),
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

fn ranked(mut picks: Vec<Pick>) -> Vec<Pick> {
    picks.sort_by(|a, b| b.p.partial_cmp(&a.p).unwrap_or(std::cmp::Ordering::Equal).then(a.id.cmp(&b.id)));
    picks
}

/// Reads Jev's answer into a route. The action is Jev's likeliest, with one correction made in
/// code: sending needs a session, so when "none" is the likelier session, new work is meant.
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
    let sessions = ranked(session_p.iter().filter_map(|(k, p)| Some(Pick { id: at(k, 's', &req.sessions)?, p: *p })).collect());
    let no_session = session_p.get("none").copied().unwrap_or(if req.sessions.is_empty() { 1.0 } else { 0.0 });
    let lanes = ranked(lane_p.iter().filter_map(|(k, p)| Some(Pick { id: at(k, 'w', &req.lanes)?, p: *p })).collect());
    let mut repo_sum: BTreeMap<String, f64> = BTreeMap::new();
    for (k, p) in &lane_p {
        if let Some(repo) = at(k, 'w', &req.lane_repos) {
            *repo_sum.entry(repo).or_default() += p;
        }
    }
    let repos = ranked(repo_sum.into_iter().map(|(id, p)| Pick { id, p }).collect());

    let duplicate = ranked(probabilities(&answers["duplicate"]).iter().filter_map(|(k, p)| Some(Pick { id: at(k, 's', &req.sessions)?, p: *p })).collect())
        .into_iter()
        .next()
        .filter(|d| d.p >= DUPLICATE_AT);

    let top_session = sessions.first().map_or(0.0, |s| s.p);
    let mut action = actions.first().map_or(Action::Start, |a| a.0);
    if matches!(action, Action::Send | Action::Find) && no_session > top_session {
        action = Action::Start;
    }

    Ok(Route {
        action,
        actions,
        sessions: sessions.into_iter().filter(|s| s.p >= SCORED_AT).take(SCORED).collect(),
        no_session,
        lanes: lanes.into_iter().take(TOP).collect(),
        repos: repos.into_iter().take(TOP).collect(),
        own_worktree: answers["own_worktree"]["noul"].as_f64().unwrap_or(0.0),
        duplicate,
        model: response["model"].as_str().unwrap_or_default().to_string(),
        ms,
        input_tokens: response["usage"]["input_tokens"].as_u64().unwrap_or(0),
    })
}

/// Routes what you typed: builds the request, asks Jev, reads the answer.
pub fn route(board: &Board, cfg: &crate::config::Config, project: &str, text: &str) -> Result<Route> {
    let req = request(board, project, text, cfg.jev.model.trim(), crate::util::now_ms())?;
    let started = Instant::now();
    let response = crate::jev::ask(&req.body)?;
    read(&req, &response, started.elapsed().as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bay, Harness, Surface};

    fn session(key: &str, title: &str, status: Status, live: bool, at: i64) -> Session {
        Session {
            key: key.into(),
            id: key.into(),
            short_id: key.into(),
            harness: Harness::Claude,
            account: "claude".into(),
            surface: Surface::Cli,
            status,
            live,
            pid: None,
            title: title.into(),
            cwd: "/r".into(),
            work: vec![],
            reply_tail: None,
            first_prompt: None,
            name: None,
            started_at: None,
            status_since: None,
            last_activity: at,
            last_prompt_at: at,
            excerpt: Some("Ran `cargo test`".into()),
            resume_command: None,
            app_account: false,
            open_target: "focus".into(),
            visitor: None,
            headline: String::new(),
            detail: None,
            branch: Some("feature".into()),
            archived: false,
            attach_command: None,
            stalled_since: None,
            background: None,
            insight: None,
        }
    }

    fn lane(id: &str, project: &str, repo: &str, sessions: Vec<Session>, at: i64) -> Lane {
        Lane {
            id: id.into(),
            project: project.into(),
            repo: repo.into(),
            repo_slug: None,
            path: id.into(),
            callsign: repo.into(),
            origin: Origin::Main,
            branch: Some("main".into()),
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
            last_activity: at,
            looks_finished: false,
            targets: vec![],
            default_target: None,
        }
    }

    const NOW: i64 = 10 * 24 * 3600 * 1000;

    fn board() -> Board {
        let old = NOW - 5 * 24 * 3600 * 1000;
        let mut archived = session("c:a", "Archived one", Status::Idle, false, NOW);
        archived.archived = true;
        let mut elsewhere = lane("/tmp/x", ELSEWHERE, "x", vec![session("c:x", "Scratch", Status::Idle, true, NOW)], NOW);
        elsewhere.hidden = true;
        Board {
            lanes: vec![
                lane("/p/api", "Acme", "api", vec![session("c:1", "Fix login", Status::Working, true, NOW - 60_000), session("c:old", "Old stopped", Status::Stopped, false, old), archived], NOW - 60_000),
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
        // Live first, then by last prompt; archived and long-stopped ones stay out.
        assert_eq!(req.sessions, vec!["c:1", "c:2"]);
        // The latest worktree first.
        assert_eq!(req.lanes, vec!["/p/web", "/p/api"]);
        assert_eq!(req.body["state"]["message"], "make the login button blue");
        assert_eq!(req.body["state"]["sessions"]["s1"]["title"], "Fix login");
        assert_eq!(req.body["state"]["sessions"]["s1"]["status"], "working on a turn right now");
        assert_eq!(req.body["state"]["sessions"]["s1"]["latest"], "Ran cargo test");
        assert_eq!(req.body["state"]["sessions"]["s1"]["last prompted"], "just now");
        assert!(req.body["questions"]["session"]["criteria"]["none"].is_string());
        assert_eq!(req.body["questions"]["worktree"]["criteria"]["w2"]["worktree"], "api (main checkout of api)");
        assert_eq!(req.body["questions"]["worktree"]["criteria"]["w2"]["recent sessions"], json!(["Fix login", "Old stopped"]));
        assert_eq!(req.body["model"], "jev-latest");
        // Only running sessions that aren't waiting on you could already be doing it.
        let dup = req.body["questions"]["duplicate"]["criteria"].as_object().unwrap();
        assert_eq!(dup.keys().collect::<Vec<_>>(), vec!["none", "s1", "s2"]);
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
        assert_eq!((r.model.as_str(), r.ms, r.input_tokens), ("jev-1.13.0", 250, 1234));
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
