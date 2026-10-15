//! What Jev does for todos (`[jev] todos`): links a new session to the todo it's for, says when
//! an open todo's work looks done, and suggests a new todo's project, labels and the todos it
//! waits on. The rules and the requests are here as plain functions with tests; the engine asks
//! (in the background, through jev.rs) and applies the answers.
//!
//! Each session is asked about once, ever, and each todo again only when something linked to it
//! has moved on (a turn ended, a PR merged): what was asked is kept in `jev-todos.json`.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::jev::{self, ago_words, clip};
use crate::model::{Board, Lane, PrState, Session, Status, TodoHint};
use crate::todos::{Label, Todo, TodoStatus};

/// A session is offered for linking while it's this new.
const LINK_WITHIN_MS: i64 = 2 * 3600 * 1000;
/// Linking needs the todo this likely…
const LINK_AT: f64 = 0.75;
/// …and this far ahead of the next likeliest answer (none included).
const LINK_LEAD: f64 = 0.3;
/// A todo shows as looking done from this likely.
pub const DONE_AT: f64 = 0.75;
/// Todos offered for linking at most.
const MAX_TODOS: usize = 60;
/// Labels and todos asked about for a new todo at most.
const MAX_LABELS: usize = 30;
const MAX_DEPS: usize = 40;

/// What's been asked and answered, kept across launches in `jev-todos.json`.
#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    /// Sessions asked about linking, answered or not.
    #[serde(default)]
    pub asked: BTreeSet<String>,
    /// Todo id → the last answer on whether it looks done.
    #[serde(default)]
    pub done: BTreeMap<String, DoneAnswer>,
    /// Todo id → the signature a "looks done" was dismissed at. A new one can suggest it again.
    #[serde(default)]
    pub dismissed: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DoneAnswer {
    /// What the todo's sessions and PRs were when it was asked.
    pub sig: String,
    pub p: f64,
    pub why: String,
}

impl Memory {
    pub fn path() -> PathBuf {
        crate::config::support_dir().join("jev-todos.json")
    }

    /// The todos' "looks done" you dismissed are yours: a jev-todos.json that doesn't read is
    /// left as it is, and a notice says so, rather than read as nothing asked and saved over at
    /// Jev's next answer (`store::load_whole`).
    pub fn load() -> Memory {
        Self::load_from(&Self::path())
    }

    /// A failure is a notice (`store::save_whole`); what's in memory is used either way.
    pub fn save(&self) {
        self.save_to(&Self::path());
    }

    fn load_from(path: &Path) -> Memory {
        crate::store::load_whole(path, "Jev's answers about todos")
    }

    fn save_to(&self, path: &Path) {
        let _ = crate::store::save_whole(path, self);
    }

    /// The todos that look done now: answered for what's linked to them now, likely enough, and
    /// not dismissed at that point.
    pub fn hints(&self, board: &Board, todos: &[Todo]) -> BTreeMap<String, TodoHint> {
        todos
            .iter()
            .filter(|t| t.status != TodoStatus::Done)
            .filter_map(|t| {
                let a = self.done.get(&t.id)?;
                let now = done_input(board, t)?;
                (a.sig == now.sig && a.p >= DONE_AT && self.dismissed.get(&t.id) != Some(&a.sig)).then(|| (t.id.clone(), TodoHint { p: a.p, why: a.why.clone() }))
            })
            .collect()
    }
}

/// Markdown images and file links out, whitespace on one line: what a description says.
fn plain_description(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find("](") {
        let open = rest[..at].rfind('[').unwrap_or(at);
        let open = if open > 0 && rest.as_bytes()[open - 1] == b'!' { open - 1 } else { open };
        let Some(close) = rest[at..].find(')') else { break };
        let label = &rest[open..at].trim_start_matches('!').trim_start_matches('[');
        out.push_str(&rest[..open]);
        if !rest[open..].starts_with('!') {
            out.push_str(label);
        }
        rest = &rest[at + close + 1..];
    }
    out.push_str(rest);
    out
}

fn status_words(s: &Session) -> &'static str {
    match s.status {
        Status::Needs => "waiting on the user",
        Status::Error => "stopped on an error",
        Status::Working => "working",
        Status::Unread => "finished its turn",
        Status::Idle => "idle, its turn ended",
        Status::Stopped => "not running",
    }
}

fn pr_words(state: PrState) -> &'static str {
    match state {
        PrState::Draft => "draft",
        PrState::Open => "open, in review",
        PrState::Approved => "approved, not merged yet",
        PrState::Changes => "changes requested",
        PrState::Merged => "merged",
        PrState::Closed => "closed without merging",
    }
}

/* ---- Linking a new session ---- */

/// Sessions to ask about: live, new, their own lane's, not linked to a todo and not asked
/// before, in a lane no start from a todo is waiting on, and saying enough about themselves.
pub fn link_candidates<'a>(board: &'a Board, todos: &[Todo], asked: &BTreeSet<String>, now: i64) -> Vec<(&'a Lane, &'a Session)> {
    let linked: HashSet<&str> = todos.iter().flat_map(|t| t.sessions.iter().map(String::as_str)).collect();
    let pending: HashSet<&str> = todos.iter().flat_map(|t| t.pending.iter().map(|p| p.lane.as_str())).collect();
    board
        .lanes
        .iter()
        .flat_map(|l| l.sessions.iter().map(move |s| (l, s)))
        .filter(|(l, s)| {
            s.visitor.is_none()
                && !s.archived
                && s.live
                && now - s.started_at.unwrap_or(s.last_prompt_at) < LINK_WITHIN_MS
                && !linked.contains(s.key.as_str())
                && !asked.contains(&s.key)
                && !pending.contains(l.id.as_str())
                && !s.title.starts_with("Session ")
                && open_todos(todos, &l.project).next().is_some()
        })
        .collect()
}

fn open_todos<'a>(todos: &'a [Todo], project: &'a str) -> impl Iterator<Item = &'a Todo> + 'a {
    todos.iter().filter(move |t| t.status != TodoStatus::Done && t.project == project)
}

fn todo_card(t: &Todo, board: &Board) -> Value {
    let mut card = Map::new();
    card.insert("title".into(), json!(clip(&t.title, 200)));
    let d = clip(&plain_description(&t.description), 400);
    if !d.is_empty() {
        card.insert("description".into(), json!(d));
    }
    let worktrees: Vec<String> = t.worktrees.iter().filter_map(|w| board.lanes.iter().find(|l| &l.id == w)).map(|l| format!("{} ({})", l.callsign, l.repo)).collect();
    if !worktrees.is_empty() {
        card.insert("worktrees".into(), json!(worktrees));
    }
    Value::Object(card)
}

/// The question for one new session, and the todo ids its options stand for (`t1` is the first).
pub fn link_request(model: &str, board: &Board, lane: &Lane, s: &Session, todos: &[Todo]) -> (Value, Vec<String>) {
    let offered: Vec<&Todo> = open_todos(todos, &lane.project).take(MAX_TODOS).collect();
    let mut options = Map::new();
    for (i, t) in offered.iter().enumerate() {
        options.insert(format!("t{}", i + 1), todo_card(t, board));
    }
    options.insert("none".into(), json!("The session's work isn't for any of these todos."));
    let mut session = Map::new();
    session.insert("title".into(), json!(clip(&s.title, 200)));
    if let Some(p) = &s.first_prompt {
        session.insert("first prompt".into(), json!(clip(p, 600)));
    }
    session.insert("repo".into(), json!(lane.repo));
    session.insert("worktree".into(), json!(lane.callsign));
    if let Some(b) = s.branch.as_deref().or(lane.branch.as_deref()) {
        session.insert("branch".into(), json!(b));
    }
    let body = jev::body(
        model,
        json!({ "session": Value::Object(session) }),
        json!({
            "todo": {
                "type": "choice",
                "instructions": "`session` is a coding agent session that just started. Which todo is it working on? Pick the todo whose work its title and first prompt describe.",
                "criteria": Value::Object(options),
            }
        }),
    );
    (body, offered.iter().map(|t| t.id.clone()).collect())
}

/// The todo to link, when the answer is sure enough and clearly ahead.
pub fn link_pick(response: &Value, ids: &[String]) -> Option<String> {
    let ranked = jev::choice(response, "todo");
    let (top, p) = ranked.first()?;
    let next = ranked.get(1).map_or(0.0, |x| x.1);
    if top == "none" || *p < LINK_AT || p - next < LINK_LEAD {
        return None;
    }
    let n: usize = top.strip_prefix('t')?.parse().ok()?;
    ids.get(n.checked_sub(1)?).cloned()
}

/* ---- Whether a todo looks done ---- */

/// What to ask about an open todo, when there's something to judge by and nothing linked is
/// still under way.
#[derive(Debug, Clone, PartialEq)]
pub struct DoneInput {
    /// Changes when a linked session starts or ends a turn, or a linked PR changes state.
    pub sig: String,
    pub state: Value,
    /// Said with the hint: a merged PR, else the latest session.
    pub why: String,
}

pub fn done_input(board: &Board, t: &Todo) -> Option<DoneInput> {
    if t.status == TodoStatus::Done {
        return None;
    }
    let mut sessions: Vec<(&Lane, &Session)> = board
        .lanes
        .iter()
        .flat_map(|l| l.sessions.iter().map(move |s| (l, s)))
        .filter(|(_, s)| s.visitor.is_none() && t.sessions.contains(&s.key))
        .collect();
    sessions.sort_by_key(|(_, s)| -s.last_prompt_at);
    let lanes: Vec<&Lane> = board.lanes.iter().filter(|l| t.worktrees.contains(&l.id) || sessions.iter().any(|(sl, _)| sl.id == l.id)).collect();
    let prs: Vec<(&Lane, &crate::model::Pr)> = lanes.iter().filter_map(|l| l.pr.as_ref().map(|p| (*l, p))).collect();
    if sessions.is_empty() && prs.is_empty() {
        return None;
    }
    if sessions.iter().any(|(_, s)| matches!(s.status, Status::Working | Status::Needs)) {
        return None;
    }
    let mut sig: Vec<String> = sessions.iter().map(|(_, s)| format!("{}@{}", s.key, s.last_prompt_at)).collect();
    sig.extend(prs.iter().map(|(_, p)| format!("#{}:{:?}", p.number, p.state)));
    sig.sort();
    let now = board.generated_at;
    let cards: Vec<Value> = sessions
        .iter()
        .take(6)
        .map(|(l, s)| {
            let mut c = Map::new();
            c.insert("title".into(), json!(clip(&s.title, 200)));
            c.insert("worktree".into(), json!(l.callsign));
            c.insert("status".into(), json!(status_words(s)));
            if let Some(r) = s.reply_tail.as_deref().or(s.excerpt.as_deref()) {
                c.insert("end of its last reply".into(), json!(jev::tail(r, 600)));
            }
            c.insert("last prompted".into(), json!(ago_words(s.last_prompt_at, now)));
            Value::Object(c)
        })
        .collect();
    let pr_cards: Vec<Value> = prs.iter().map(|(l, p)| json!({ "title": clip(&format!("#{} {}", p.number, p.title), 200), "worktree": l.callsign, "state": pr_words(p.state) })).collect();
    let why = match prs.iter().find(|(_, p)| p.state == PrState::Merged) {
        Some((_, p)) => format!("#{} merged", p.number),
        None => match sessions.first() {
            Some((_, s)) => format!("“{}” finished its turn", clip(&s.title, 80)),
            None => "Its pull request".to_string(),
        },
    };
    let mut todo = Map::new();
    todo.insert("title".into(), json!(clip(&t.title, 200)));
    let d = clip(&plain_description(&t.description), 600);
    if !d.is_empty() {
        todo.insert("description".into(), json!(d));
    }
    Some(DoneInput { sig: sig.join(" "), state: json!({ "todo": Value::Object(todo), "sessions": cards, "pull requests": pr_cards }), why })
}

pub fn done_request(model: &str, input: &DoneInput) -> Value {
    jev::body(
        model,
        input.state.clone(),
        json!({
            "done": {
                "type": "noul",
                "instructions": "Is all of the work that `todo` describes finished, going by `sessions` and `pull requests`?",
                "criteria": {
                    "true": "They show the work was completed: its pull request merged, or a session reports it done, landed or shipped.",
                    "false": "Work remains, a session is still on it or stopped partway, or they don't say it's finished.",
                },
            }
        }),
    )
}

/* ---- A new todo's project, labels and what it waits on ---- */

/// What Jev suggests for a todo, each with how likely it is, the likeliest first.
#[derive(Serialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Suggested {
    pub projects: Vec<(String, f64)>,
    pub labels: Vec<(String, f64)>,
    pub depends_on: Vec<(String, f64)>,
}

/// The request, and the label and todo ids its `label_N` and `dep_N` questions stand for.
pub struct SuggestRequest {
    pub body: Value,
    /// The images in the todo's description, sent with it (`suggest`).
    images: jev::Images,
    projects: bool,
    labels: Vec<String>,
    deps: Vec<String>,
}

/// What Jev suggests for a todo being written, on this board: its project, labels and the
/// todos it waits on. An error when `[jev] todos` is off or there's nothing to ask.
pub fn suggest(board: &Board, id: Option<&str>, title: &str, description: &str, project: &str) -> anyhow::Result<Suggested> {
    let cfg = crate::config::current()?;
    if !cfg.jev.todos {
        anyhow::bail!("Todo suggestions are off.");
    }
    let projects: Vec<String> = board.projects.iter().map(|p| p.name.clone()).filter(|p| p != crate::board::ELSEWHERE).collect();
    let mut req = suggest_request(cfg.jev.model.trim(), id, title, description, project, &projects, &board.labels, &board.todos, board).ok_or_else(|| anyhow::anyhow!("Nothing to suggest."))?;
    req.images.attach(&mut req.body);
    Ok(suggest_read(&req, &jev::ask(&req.body)?))
}

pub fn suggest_request(model: &str, id: Option<&str>, title: &str, description: &str, project: &str, projects: &[String], labels: &[Label], todos: &[Todo], board: &Board) -> Option<SuggestRequest> {
    if title.trim().is_empty() {
        return None;
    }
    let mut questions = Map::new();
    let ask_project = projects.len() > 1;
    if ask_project {
        let mut options = Map::new();
        for p in projects {
            let repos: BTreeSet<&str> = board.lanes.iter().filter(|l| &l.project == p).map(|l| l.repo.as_str()).collect();
            let recent: Vec<String> = todos.iter().filter(|t| &t.project == p).take(5).map(|t| clip(&t.title, 120)).collect();
            options.insert(p.clone(), json!({ "repos": repos.into_iter().take(10).collect::<Vec<_>>(), "some of its todos": recent }));
        }
        questions.insert(
            "project".into(),
            json!({ "type": "choice", "instructions": "Which project does the work in `todo` belong to? Match the product, repo or feature it names.", "criteria": Value::Object(options) }),
        );
    }
    let label_ids: Vec<String> = labels.iter().filter(|l| !l.name.trim().is_empty()).take(MAX_LABELS).map(|l| l.id.clone()).collect();
    for (i, lid) in label_ids.iter().enumerate() {
        let name = labels.iter().find(|l| &l.id == lid).map(|l| l.name.as_str()).unwrap_or_default();
        questions.insert(format!("label_{}", i + 1), json!({ "type": "noul", "instructions": format!("Does the label '{name}' fit `todo`?") }));
    }
    let deps: Vec<&Todo> = todos
        .iter()
        .filter(|t| t.status != TodoStatus::Done && Some(t.id.as_str()) != id && (project.is_empty() || t.project == project) && !t.title.trim().is_empty())
        .take(MAX_DEPS)
        .collect();
    for (i, t) in deps.iter().enumerate() {
        questions.insert(
            format!("dep_{}", i + 1),
            json!({
                "type": "noul",
                "instructions": format!("Must the todo '{}' be finished before work on `todo` can start?", clip(&t.title, 160)),
                "criteria": { "true": "`todo` builds on it or can't be done without it.", "false": "They're separate, or `todo` could be done first or at the same time." },
            }),
        );
    }
    if questions.is_empty() {
        return None;
    }
    let mut todo = Map::new();
    todo.insert("title".into(), json!(clip(title, 200)));
    // Its images go where they are in it; a todo you're writing is often a screenshot and a line.
    let mut images = jev::Images::default();
    let d = clip(&plain_description(&images.swap(description)), 800);
    if !d.is_empty() {
        todo.insert("description".into(), json!(d));
    }
    Some(SuggestRequest {
        body: jev::body(model, json!({ "todo": Value::Object(todo) }), Value::Object(questions)),
        images,
        projects: ask_project,
        labels: label_ids,
        deps: deps.iter().map(|t| t.id.clone()).collect(),
    })
}

pub fn suggest_read(req: &SuggestRequest, response: &Value) -> Suggested {
    let sorted = |mut v: Vec<(String, f64)>| {
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        v
    };
    let nouls = |prefix: &str, ids: &[String]| sorted(ids.iter().enumerate().filter_map(|(i, id)| Some((id.clone(), jev::noul(response, &format!("{prefix}_{}", i + 1))?))).collect());
    Suggested {
        projects: if req.projects { jev::choice(response, "project") } else { Vec::new() },
        labels: nouls("label", &req.labels),
        depends_on: nouls("dep", &req.deps),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Origin, Pr};

    const NOW: i64 = 100 * 24 * 3600 * 1000;

    fn session(key: &str, title: &str, status: Status, started: i64) -> Session {
        Session { status, title: title.into(), started_at: Some(started), last_activity: started, last_prompt_at: started, excerpt: Some("All done: the export now streams.".into()), ..crate::fixtures::session(key) }
    }

    fn lane(id: &str, project: &str, sessions: Vec<Session>, pr: Option<Pr>) -> Lane {
        Lane { project: project.into(), repo: "api".into(), origin: Origin::Script, branch: Some("feature".into()), pr, last_activity: NOW, ..crate::fixtures::lane(id, sessions) }
    }

    fn todo(id: &str, project: &str, title: &str) -> Todo {
        serde_json::from_value(json!({ "id": id, "project": project, "title": title })).unwrap()
    }

    fn pr(state: PrState) -> Pr {
        Pr { number: 7, title: "Stream exports".into(), url: String::new(), state, checks: Default::default(), merge: Default::default(), updated_at: 0, merged_at: None, closed_at: None }
    }

    fn board(lanes: Vec<Lane>) -> Board {
        Board { lanes, generated_at: NOW, ..Default::default() }
    }

    #[test]
    fn offers_new_unlinked_sessions_in_projects_with_open_todos() {
        let mut old = session("c:old", "Old work", Status::Idle, NOW - 5 * 3600 * 1000);
        old.live = true;
        let b = board(vec![
            lane("/a", "Acme", vec![session("c:new", "Stream the export", Status::Working, NOW - 60_000), session("c:linked", "Linked", Status::Working, NOW), session("c:asked", "Asked", Status::Working, NOW), old], None),
            lane("/b", "Blog", vec![session("c:blog", "Write a post", Status::Working, NOW)], None),
            lane("/c", "Acme", vec![session("c:pending", "Pending start", Status::Working, NOW), session("c:unnamed", "Session 1a2b3c4d", Status::Working, NOW)], None),
        ]);
        let mut linked = todo("t1", "Acme", "Export streaming");
        linked.sessions = vec!["c:linked".into()];
        let mut waiting = todo("t2", "Acme", "Other");
        waiting.pending = vec![serde_json::from_value(json!({ "lane": "/c", "harness": "claude", "account": "claude", "at": NOW })).unwrap()];
        let asked: BTreeSet<String> = ["c:asked".to_string()].into();
        let got: Vec<&str> = link_candidates(&b, &[linked, waiting], &asked, NOW).iter().map(|(_, s)| s.key.as_str()).collect();
        // The blog has no todos; old, linked, asked, pending-lane and unnamed sessions stay out.
        assert_eq!(got, vec!["c:new"]);
    }

    #[test]
    fn asks_which_open_todo_and_links_only_a_clear_answer() {
        let b = board(vec![lane("/a", "Acme", vec![], None)]);
        let mut s = session("c:new", "Stream the export", Status::Working, NOW);
        s.first_prompt = Some("make the CSV export stream rows".into());
        let mut done = todo("t0", "Acme", "Finished thing");
        done.status = TodoStatus::Done;
        let todos = vec![done, todo("t1", "Acme", "Export streaming"), todo("t2", "Acme", "Fix login"), todo("t3", "Blog", "Post")];
        let (body, ids) = link_request("m", &b, &b.lanes[0], &s, &todos);
        assert_eq!(ids, vec!["t1", "t2"]);
        assert_eq!(body["questions"]["todo"]["criteria"]["t1"]["title"], "Export streaming");
        assert!(body["questions"]["todo"]["criteria"]["none"].is_string());
        assert_eq!(body["state"]["session"]["first prompt"], "make the CSV export stream rows");
        let answer = |t1: f64, t2: f64, none: f64| json!({ "answers": { "todo": { "probabilities": { "t1": t1, "t2": t2, "none": none } } } });
        assert_eq!(link_pick(&answer(0.9, 0.05, 0.05), &ids).as_deref(), Some("t1"));
        assert_eq!(link_pick(&answer(0.7, 0.1, 0.2), &ids), None, "not sure enough");
        assert_eq!(link_pick(&answer(0.1, 0.1, 0.8), &ids), None, "none");
    }

    #[test]
    fn judges_done_once_nothing_linked_is_under_way() {
        let mut working = session("c:1", "Stream the export", Status::Working, NOW - 1000);
        let b = board(vec![lane("/a", "Acme", vec![working.clone()], Some(pr(PrState::Open)))]);
        let mut t = todo("t1", "Acme", "Export streaming");
        t.sessions = vec!["c:1".into()];
        assert_eq!(done_input(&b, &t), None, "still working");

        working.status = Status::Unread;
        working.reply_tail = Some("…so it's merged and deployed.".into());
        let b = board(vec![lane("/a", "Acme", vec![working.clone()], Some(pr(PrState::Open)))]);
        let open = done_input(&b, &t).unwrap();
        assert_eq!(open.why, "“Stream the export” finished its turn");
        assert_eq!(open.state["sessions"][0]["end of its last reply"], "…so it's merged and deployed.");
        assert_eq!(open.state["pull requests"][0]["state"], "open, in review");
        let merged = done_input(&board(vec![lane("/a", "Acme", vec![working], Some(pr(PrState::Merged)))]), &t).unwrap();
        assert_eq!(merged.why, "#7 merged");
        assert_ne!(open.sig, merged.sig, "a merge is news");
        assert_eq!(done_request("m", &merged)["questions"]["done"]["type"], "noul");

        let mut plain = todo("t2", "Acme", "Nothing linked");
        plain.worktrees = vec!["/nowhere".into()];
        assert_eq!(done_input(&b, &plain), None);
    }

    #[test]
    fn shows_a_hint_for_what_was_asked_until_dismissed_or_outdated() {
        let mut s = session("c:1", "Stream the export", Status::Unread, NOW - 1000);
        let mut t = todo("t1", "Acme", "Export streaming");
        t.sessions = vec!["c:1".into()];
        let b = board(vec![lane("/a", "Acme", vec![s.clone()], None)]);
        let sig = done_input(&b, &t).unwrap().sig;
        let mut m = Memory::default();
        m.done.insert("t1".into(), DoneAnswer { sig: sig.clone(), p: 0.9, why: "w".into() });
        assert_eq!(m.hints(&b, std::slice::from_ref(&t)).get("t1"), Some(&TodoHint { p: 0.9, why: "w".into() }));
        m.dismissed.insert("t1".into(), sig);
        assert!(m.hints(&b, std::slice::from_ref(&t)).is_empty(), "dismissed");
        m.dismissed.clear();
        s.last_prompt_at += 5000;
        let moved = board(vec![lane("/a", "Acme", vec![s], None)]);
        assert!(m.hints(&moved, std::slice::from_ref(&t)).is_empty(), "asked about an older turn");
    }

    #[test]
    fn asks_about_projects_labels_and_what_it_waits_on() {
        let b = board(vec![lane("/a", "Acme", vec![], None), lane("/b", "Blog", vec![], None)]);
        let labels: Vec<Label> = serde_json::from_value(json!([{ "id": "l1", "name": "bug", "color": "red" }, { "id": "l2", "name": "infra", "color": "blue" }])).unwrap();
        let todos = vec![todo("me", "Acme", "This one"), todo("t1", "Acme", "Set up the queue"), todo("t2", "Blog", "Elsewhere")];
        let projects = vec!["Acme".to_string(), "Blog".to_string()];
        let req = suggest_request("m", Some("me"), "Retry failed jobs ![shot](images/a.png)", "", "Acme", &projects, &labels, &todos, &b).unwrap();
        let q = &req.body["questions"];
        assert_eq!(q["label_2"]["instructions"], "Does the label 'infra' fit `todo`?");
        assert!(q["dep_1"]["instructions"].as_str().unwrap().contains("'Set up the queue'"));
        assert!(q["dep_2"].is_null(), "only open todos in the project, not itself");
        assert_eq!(req.body["state"]["todo"]["title"], "Retry failed jobs ![shot](images/a.png)");
        let answer = json!({ "answers": {
            "project": { "probabilities": { "Acme": 0.9, "Blog": 0.1 } },
            "label_1": { "noul": 0.2 }, "label_2": { "noul": 0.8 }, "dep_1": { "noul": 0.85 },
        }});
        let s = suggest_read(&req, &answer);
        assert_eq!(s.projects[0], ("Acme".to_string(), 0.9));
        assert_eq!(s.labels, vec![("l2".to_string(), 0.8), ("l1".to_string(), 0.2)]);
        assert_eq!(s.depends_on, vec![("t1".to_string(), 0.85)]);
        // One project and no labels or other todos leaves nothing to ask.
        assert!(suggest_request("m", None, "x", "", "Acme", &projects[..1], &[], &[], &b).is_none());
    }

    #[test]
    fn a_jev_todos_json_that_does_not_read_is_left_as_it_is_and_says_so() {
        let dir = crate::store::test_dir("jev-todos");
        let path = dir.join("jev-todos.json");
        // A dismissal, saved and read back.
        let mut m = Memory::default();
        m.dismissed.insert("t1".into(), "sig".into());
        m.save_to(&path);
        assert_eq!(Memory::load_from(&path), m);
        // Another build's shape, or a write cut short: Jev starts from nothing asked, but the
        // file, with the dismissals in it, isn't saved over at its next answer, and a notice
        // says why.
        let broken = r#"{"asked": ["a"], "dismissed": {"t1": 7"#;
        std::fs::write(&path, broken).unwrap();
        assert_eq!(Memory::load_from(&path), Memory::default());
        let mut next = Memory::default();
        next.asked.insert("b".into());
        next.save_to(&path);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
        let problems = crate::store::problems(&dir);
        assert!(problems.iter().any(|(f, why)| f == "jev-todos.json" && why.starts_with("jev-todos.json doesn't read as Jev's answers about todos")), "{problems:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn descriptions_lose_their_images_and_keep_link_text() {
        assert_eq!(plain_description("See ![shot](images/a.png) and [the doc](files/x/doc.pdf) now"), "See  and the doc now");
    }

    #[test]
    fn a_new_todos_images_go_where_they_are_in_its_description() {
        let b = board(vec![lane("/a", "Acme", vec![], None), lane("/b", "Blog", vec![], None)]);
        let projects = vec!["Acme".to_string(), "Blog".to_string()];
        let req = suggest_request("m", None, "Bar overlaps", "Like this ![image](images/0123456789abcdef.png)\n[the log](files/x/a.log)", "", &projects, &[], &[], &b).unwrap();
        assert_eq!(req.body["state"]["todo"]["description"], "Like this image-0123456789abcdef the log");
    }
}
