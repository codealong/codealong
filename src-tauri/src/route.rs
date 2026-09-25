//! Routing for the quick panel: what you type goes to TypeSafe's Jev, which answers a few typed
//! questions about it at once (docs.typesafe.ai): is it for a session that's already going, new
//! work, a search, or a note for later; which session; which worktree; and does it want a
//! worktree of its own. Jev picks from lists and gives a probability for every option, so the
//! code here decides what to suggest and how sure to sound, and the panel only acts on ⏎.
//!
//! Jev reads questions literally and is weak at dates and numbers, so times go in as words
//! ("5 minutes ago") and each question asks one plain thing.
//!
//! The API key lives in the login keychain (service `Codealong TypeSafe`), never in config.toml
//! or on a command line: curl gets it and the request on stdin, and saving it goes through
//! `security -i`, which reads its command from stdin too. `TYPESAFE_API_KEY` works when there's
//! no key in the keychain, for the command line.

use std::collections::BTreeMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::board::ELSEWHERE;
use crate::model::{Board, Lane, Origin, Session, Status};

const ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
const KEYCHAIN_SERVICE: &str = "Codealong TypeSafe";
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
    /// Session keys, most likely first; the top ones only.
    pub sessions: Vec<Pick>,
    /// How likely it is that no listed session is meant.
    pub no_session: f64,
    /// Worktrees (lane ids), most likely first; the top ones only.
    pub lanes: Vec<Pick>,
    /// Repos, the worktrees' probabilities added up: which repo is clearer than which worktree.
    pub repos: Vec<Pick>,
    /// How likely it is that new work wants a worktree of its own.
    pub own_worktree: f64,
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

/// Whether a lane is in the panel's project, as the panel decides it (quick.ts `inProject`).
fn in_project(lane: &Lane, project: &str) -> bool {
    if project == ALL_PROJECTS {
        return !lane.hidden;
    }
    lane.project == project && (project == ELSEWHERE || !lane.hidden)
}

/// A time as words. Jev reads dates as text and can't compare them; words it can.
pub fn ago_words(then: i64, now: i64) -> String {
    let mins = (now - then).max(0) / 60_000;
    match mins {
        0..=1 => "just now".into(),
        2..=59 => format!("{mins} minutes ago"),
        60..=119 => "an hour ago".into(),
        120..=1439 => format!("{} hours ago", mins / 60),
        1440..=2879 => "yesterday".into(),
        _ => format!("{} days ago", mins / 1440),
    }
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
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= TEXT_CHARS {
        return s;
    }
    let mut out: String = s.chars().take(TEXT_CHARS - 1).collect();
    out.push('…');
    out
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
    card.insert("last prompted".into(), json!(ago_words(s.last_prompt_at, now)));
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
    card.insert("last active".into(), json!(ago_words(l.last_activity, now)));
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
                "instructions": "Which repository worktree does the work in `message` belong in? Match the product, repo, feature or files it talks about.",
                "criteria": Value::Object(lane_cards),
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

    let top_session = sessions.first().map_or(0.0, |s| s.p);
    let mut action = actions.first().map_or(Action::Start, |a| a.0);
    if matches!(action, Action::Send | Action::Find) && no_session > top_session {
        action = Action::Start;
    }

    Ok(Route {
        action,
        actions,
        sessions: sessions.into_iter().take(TOP).collect(),
        no_session,
        lanes: lanes.into_iter().take(TOP).collect(),
        repos: repos.into_iter().take(TOP).collect(),
        own_worktree: answers["own_worktree"]["noul"].as_f64().unwrap_or(0.0),
        model: response["model"].as_str().unwrap_or_default().to_string(),
        ms,
        input_tokens: response["usage"]["input_tokens"].as_u64().unwrap_or(0),
    })
}

/// Routes what you typed: builds the request, asks Jev, reads the answer.
pub fn route(board: &Board, cfg: &crate::config::Config, project: &str, text: &str) -> Result<Route> {
    let key = api_key().ok_or_else(|| anyhow!("There's no TypeSafe API key yet. Add it in Settings › General."))?;
    let req = request(board, project, text, cfg.router.model.trim(), crate::util::now_ms())?;
    let started = Instant::now();
    let response = post(&key, &req.body)?;
    read(&req, &response, started.elapsed().as_millis() as u64)
}

/// One call with a tiny question, for Settings' Test button: proves the key and says how fast.
pub fn check(model: &str) -> Result<String> {
    let key = api_key().ok_or_else(|| anyhow!("There's no TypeSafe API key yet."))?;
    let started = Instant::now();
    let v = post(
        &key,
        &json!({
            "model": model,
            "state": "Fix the flaky login test",
            "questions": { "task": { "type": "noul", "instructions": "Does this ask for a change to code?" } },
        }),
    )?;
    Ok(format!("{} answered in {} ms.", v["model"].as_str().unwrap_or(model), started.elapsed().as_millis()))
}

/// Escapes a value for a double-quoted string in a curl config file.
fn curl_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The curl config for one request: the key and the body both go on curl's stdin.
fn curl_config(key: &str, body: &Value) -> String {
    format!(
        "url = {}\nrequest = \"POST\"\nheader = {}\nheader = \"Content-Type: application/json\"\nheader = \"Accept: application/json\"\nheader = \"User-Agent: codealong/{}\"\ndata-binary = {}\n",
        curl_quoted(ENDPOINT),
        curl_quoted(&format!("Authorization: Bearer {key}")),
        env!("CARGO_PKG_VERSION"),
        curl_quoted(&body.to_string()),
    )
}

fn post(key: &str, body: &Value) -> Result<Value> {
    if key.chars().any(|c| c.is_control() || c == '"' || c == '\\') {
        bail!("The TypeSafe API key has characters a key can't have. Paste it again in Settings.");
    }
    let mut child = Command::new("/usr/bin/curl")
        .args(["-sS", "-m", "15", "-w", "\n%{http_code}", "-K", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("curl: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(curl_config(key, body).as_bytes())?;
    }
    let out = child.wait_with_output().map_err(|e| anyhow!("curl: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let (reply, status) = text.rsplit_once('\n').unwrap_or(("", &text));
    let detail = || -> String {
        serde_json::from_str::<Value>(reply)
            .ok()
            .and_then(|v| v.get("detail").or_else(|| v.get("error")).or_else(|| v.get("message")).map(|d| d.to_string()))
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect()
    };
    match status.trim() {
        "200" => serde_json::from_str(reply).map_err(|_| anyhow!("TypeSafe sent an answer Codealong can't read.")),
        "401" | "403" => bail!("TypeSafe didn't accept the API key. Check it in Settings › General."),
        "422" => bail!("TypeSafe couldn't take the request: {}", detail()),
        "429" => bail!("TypeSafe is rate limiting; try again in a moment."),
        "529" => bail!("TypeSafe is overloaded; try again in a moment."),
        "000" | "" => {
            let err = String::from_utf8_lossy(&out.stderr);
            bail!("Couldn't reach TypeSafe: {}", err.trim().trim_start_matches("curl: ").chars().take(160).collect::<String>())
        }
        other => bail!("TypeSafe answered HTTP {other}. {}", detail()),
    }
}

/* ---- The key ---- */

const KEYCHAIN_TIMEOUT: Duration = Duration::from_secs(10);

fn keychain_account() -> String {
    std::env::var("USER").ok().filter(|u| !u.is_empty() && u.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))).unwrap_or_else(|| "codealong".into())
}

/// The API key: the keychain's, else `TYPESAFE_API_KEY`.
pub fn api_key() -> Option<String> {
    crate::procs::run("/usr/bin/security", &["find-generic-password", "-a", &keychain_account(), "-s", KEYCHAIN_SERVICE, "-w"], None, KEYCHAIN_TIMEOUT)
        .ok()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .or_else(|| std::env::var("TYPESAFE_API_KEY").ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty()))
}

/// Whether there's a key, without reading it.
pub fn has_key() -> bool {
    crate::procs::run("/usr/bin/security", &["find-generic-password", "-a", &keychain_account(), "-s", KEYCHAIN_SERVICE], None, KEYCHAIN_TIMEOUT).is_ok()
        || std::env::var("TYPESAFE_API_KEY").is_ok_and(|k| !k.trim().is_empty())
}

/// Saves the key in the keychain, replacing one that's there. The command goes to `security`
/// on stdin, so the key never shows in the process list.
pub fn save_key(key: &str) -> Result<()> {
    let key = key.trim();
    if key.is_empty() || key.chars().any(|c| c.is_control() || c.is_whitespace() || c == '"' || c == '\\') {
        bail!("That doesn't look like an API key.");
    }
    let command = format!("add-generic-password -U -a \"{}\" -s \"{}\" -l \"{}\" -w \"{}\"\n", keychain_account(), KEYCHAIN_SERVICE, KEYCHAIN_SERVICE, key);
    let out = crate::procs::run_stdin("/usr/bin/security", &["-i"], &command, KEYCHAIN_TIMEOUT)?;
    // `security -i` exits cleanly whatever its command did; errors come back as text.
    if out.to_lowercase().contains("error") {
        bail!("The keychain didn't take the key.");
    }
    if !has_key() {
        bail!("The keychain didn't take the key.");
    }
    Ok(())
}

pub fn delete_key() -> Result<()> {
    let _ = crate::procs::run("/usr/bin/security", &["delete-generic-password", "-a", &keychain_account(), "-s", KEYCHAIN_SERVICE], None, KEYCHAIN_TIMEOUT);
    Ok(())
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
    fn times_go_in_as_words() {
        assert_eq!(ago_words(NOW - 30_000, NOW), "just now");
        assert_eq!(ago_words(NOW - 5 * 60_000, NOW), "5 minutes ago");
        assert_eq!(ago_words(NOW - 3 * 3_600_000, NOW), "3 hours ago");
        assert_eq!(ago_words(NOW - 30 * 3_600_000, NOW), "yesterday");
        assert_eq!(ago_words(NOW - 4 * 86_400_000, NOW), "4 days ago");
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

    #[test]
    fn the_key_and_body_are_escaped_for_curl() {
        let body = json!({ "state": "say \"hi\"\nback\\slash" });
        let cfg = curl_config("tsk_123", &body);
        assert!(cfg.contains("header = \"Authorization: Bearer tsk_123\"\n"));
        assert!(cfg.contains(r#"data-binary = "{\"state\":\"say \\\"hi\\\"\\nback\\\\slash\"}""#), "{cfg}");
        assert!(cfg.starts_with("url = \"https://api.typesafe.ai/v1/systemone\"\n"));
    }
}
