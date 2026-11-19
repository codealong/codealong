//! `codealong board` and `codealong canvas`: a project's boards (boards.rs), as Kanban and as
//! Canvas. Boards are off until `[experimental] boards` is on (`boards_on`).
//!
//! - A change to a board is a `BoardOp` (`apply_board`): through the server when it runs, so the
//!   windows show it at once, else in the file. What only the main window works out (which cards
//!   a rule takes in, where they're drawn) is asked of it (`ask_board`), so `show`, `move` and
//!   `arrange` need the app open.
//! - A card is given as the command names its thing (a todo's id, a session's key, a worktree's
//!   path, a PR) and written as boards key it (`card_ids`).
//! - The file isn't `boards.rs` because cli.rs names the engine's `boards` module bare, nor
//!   `board.rs` because this file and session.rs name the engine's `board` the same way.

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};

use crate::board;
use crate::boards::{self, BoardDoc, BoardOp, Rule};
use crate::config::Config;
use crate::model::Board;
use crate::server::{self, Request};
use crate::start::lane_for_path;
use crate::todos::{Label, TodoStore, LABEL_COLORS};
use crate::util::{now_ms, truncate};

use super::*;

// ---- boards ----

pub(super) fn board(cmd: BoardCmd, c: &mut Client, json: bool) -> Result<()> {
    boards_on(c.cfg()?)?;
    match cmd {
        BoardCmd::List(at) => board_list(c, &at, json),
        BoardCmd::New(a) => board_new(c, a, json),
        BoardCmd::Show(a) => board_show(c, a, json),
        BoardCmd::Add(a) => board_cards(c, a, true, json),
        BoardCmd::Remove(a) => board_cards(c, a, false, json),
        BoardCmd::Rule(a) => board_rule(c, a, json),
        BoardCmd::Set(a) => board_set(c, a, json),
        BoardCmd::Move(a) => board_move(c, a, json),
        BoardCmd::Arrange(a) => board_arrange(c, a, json),
        BoardCmd::Open(a) => {
            let b = find_board(c, a.board.as_deref(), &a.at)?;
            c.show(json!({ "kind": "board", "board": b.id, "project": b.project, "layout": a.layout }))?;
            say_done(json, &b, format!("Showed {} as {}.\n", b.name, lower_debug(&a.layout)))
        }
        BoardCmd::Rename { board, name, at } => {
            let b = find_board(c, Some(&board), &at)?;
            let out = apply_board(c, BoardOp::Rename { id: b.id.clone(), name })?;
            let now = out.board.unwrap_or(b);
            say_done(json, &now, format!("Renamed the board {board} to {}.\n", now.name))
        }
        BoardCmd::Project { board, to, at } => {
            let b = find_board(c, Some(&board), &at)?;
            let to = board_project(c, &BoardAt { project: Some(to) })?.ok_or_else(|| anyhow!("Name the project to move it to."))?;
            let now = apply_board(c, BoardOp::Project { id: b.id.clone(), project: to })?.board.unwrap_or(b);
            // What it has by hand stays on it, though another project's won't show there.
            let added = now.added.clone();
            say_unshown(c, json, &now, &added, format!("Moved {} to {}.\n", now.name, now.project))
        }
        BoardCmd::Delete { board, at } => {
            let b = find_board(c, Some(&board), &at)?;
            apply_board(c, BoardOp::Delete { id: b.id.clone() })?;
            say_done(json, &b, format!("Deleted the board {} from {}.\n", b.name, b.project))
        }
    }
}

/// Boards are an experiment: while it's off the app shows none, so a command that made or
/// changed one would do so out of sight.
fn boards_on(cfg: &Config) -> Result<()> {
    if !cfg.experimental.boards {
        bail!("Boards are an experiment, and they're turned off. Turn them on in Settings › Experimental, or with `boards = true` under `[experimental]` in config.toml.");
    }
    Ok(())
}

/// What a change to a board says: the board as JSON, or a line.
fn say_done(json: bool, b: &BoardDoc, line: String) -> Result<()> {
    if json {
        return print_json(b);
    }
    print!("{line}");
    Ok(())
}

/// The project a board command is about: --project as the app names it (`all` for All projects),
/// else the project of the worktree the current folder is in.
fn board_project(c: &mut Client, at: &BoardAt) -> Result<Option<String>> {
    let Some(asked) = at.project.as_deref().map(str::trim) else {
        return Ok(std::env::current_dir().ok().and_then(|d| lane_for_path(c.board().ok()?, &d).map(|l| l.project.clone())).filter(|p| !p.is_empty()));
    };
    if asked.eq_ignore_ascii_case("all") || asked.eq_ignore_ascii_case(crate::route::ALL_PROJECTS) {
        return Ok(Some(crate::route::ALL_PROJECTS.to_string()));
    }
    let projects: Vec<String> = c.board()?.projects.iter().map(|p| p.name.clone()).collect();
    match projects.iter().find(|p| p.eq_ignore_ascii_case(asked)) {
        Some(p) => Ok(Some(p.clone())),
        None => bail!("There's no project {asked}. The projects are {}, and all.", projects.join(", ")),
    }
}

/// The board a command names: by its id, by its name in the project it's about, or by a name only
/// one project has. None names the one board the project has.
fn find_board(c: &mut Client, name: Option<&str>, at: &BoardAt) -> Result<BoardDoc> {
    let project = board_project(c, at)?;
    let Some(name) = name.map(str::trim).filter(|n| !n.is_empty()) else {
        let p = project.ok_or_else(|| anyhow!("Name a board, or give --project: this folder isn't in a worktree of a project."))?;
        return only_board(&p, boards::list(&p)?);
    };
    let stored = boards::all()?;
    if let Some(b) = stored.iter().find(|b| b.id == name) {
        return Ok(b.clone());
    }
    let listed = match &project {
        Some(p) => boards::list(p)?,
        None => vec![],
    };
    board_named(name, project.as_deref(), listed, stored)
}

/// The board of a project that has just one (`listed`, its boards as `boards::list` gives them),
/// for a command that names none.
fn only_board(project: &str, mut listed: Vec<BoardDoc>) -> Result<BoardDoc> {
    // All projects lists every project's boards too, but only its own are its.
    listed.retain(|b| b.project == project);
    match listed.len() {
        1 => Ok(listed.remove(0)),
        0 => bail!("{project} has no boards. Make one with `codealong board new <name>`, with --everything for one that holds everything in it, once your user asks for one."),
        n => bail!("{project} has {n} boards: name one ({}).", listed.iter().map(|b| b.name.as_str()).collect::<Vec<_>>().join(", ")),
    }
}

/// The board a name names: the project's own of that name (`listed`, its boards as `boards::list`
/// gives them), else the one board of that name in any project; two of them are turned down, as
/// All projects lists every project's.
fn board_named(name: &str, project: Option<&str>, listed: Vec<BoardDoc>, stored: Vec<BoardDoc>) -> Result<BoardDoc> {
    let named = |b: &BoardDoc| b.name.eq_ignore_ascii_case(name);
    if let Some(p) = project {
        if let Some(b) = listed.into_iter().find(|b| b.project == p && named(b)) {
            return Ok(b);
        }
    }
    let mut hits: Vec<BoardDoc> = stored.into_iter().filter(named).collect();
    match hits.len() {
        1 => Ok(hits.remove(0)),
        0 => bail!("There's no board {name}{}. `codealong board list` lists a project's boards.", project.map(|p| format!(" in {p}")).unwrap_or_default()),
        n => bail!("{n} projects have a board called {name}: give --project, or the board's id ({}).", hits.iter().map(|b| b.id.as_str()).collect::<Vec<_>>().join(", ")),
    }
}

/// Makes a change to a board: through the app when it runs, so its windows show it at once, else
/// in the file.
fn apply_board(c: &mut Client, op: BoardOp) -> Result<boards::Applied> {
    c.note();
    if c.app.is_none() {
        return boards::apply(op, now_ms());
    }
    match server::ask(&Request::BoardApply { op }) {
        Some(Ok(v)) => serde_json::from_value(v).context("reading the app's reply"),
        Some(Err(e)) if e.starts_with(server::UNKNOWN_REQUEST) => bail!("The Codealong that's running is older than this command and has no boards. Update it, or quit it and open it again."),
        Some(Err(e)) => bail!("{e}"),
        None => bail!("The app stopped answering."),
    }
}

/// One of the main window's board requests (boards.rs `ask`).
fn ask_board(c: &mut Client, board: &BoardDoc, ask: boards::Ask) -> Result<Value> {
    c.note();
    if c.app.is_none() {
        bail!("Codealong isn't running, and its main window works boards out.");
    }
    match server::ask(&Request::BoardAsk { board: board.id.clone(), project: board.project.clone(), ask }) {
        Some(Ok(v)) => Ok(v),
        Some(Err(e)) if e.starts_with(server::UNKNOWN_REQUEST) => bail!("The Codealong that's running is older than this command and has no boards. Update it, or quit it and open it again."),
        Some(Err(e)) => bail!("{e}"),
        None => bail!("The app stopped answering."),
    }
}

/// The cards the options name, as boards name them (`boards::card_id`). Every one that can't be
/// named is said at once.
fn card_ids(c: &mut Client, a: &CardArgs) -> Result<Vec<String>> {
    let todos = store_of(c.board()?);
    let mut out = Vec::new();
    let mut wrong = Vec::new();
    let mut add = |kind: boards::Kind, key: Result<String>| match key {
        Ok(key) => out.push(boards::card_id(kind, &key)),
        Err(e) => wrong.push(e.to_string()),
    };
    for t in &a.todos {
        add(boards::Kind::Todo, resolve_todo(&todos, t));
    }
    for s in &a.sessions {
        add(boards::Kind::Session, resolve_session(c, s));
    }
    for w in &a.worktrees {
        add(boards::Kind::Worktree, resolve_worktree(c, w));
    }
    for p in &a.prs {
        add(boards::Kind::Pr, board_pr(c, p));
    }
    if !wrong.is_empty() {
        bail!("{}", wrong.join("\n"));
    }
    Ok(out)
}

/// A PR's key as the board spells it: GitHub's names ignore case, and the window's keys don't.
fn board_pr(c: &mut Client, pr: &str) -> Result<String> {
    resolve_pr(c, pr).map(|key| c.board().ok().and_then(|b| find_pr(b, &key)).map_or(key, |pr| pr.key.clone()))
}

fn board_cards(c: &mut Client, a: BoardCardsArgs, add: bool, json: bool) -> Result<()> {
    let b = find_board(c, Some(&a.board), &a.at)?;
    let cards = card_ids(c, &a.cards)?;
    if cards.is_empty() {
        bail!("Name the cards: --todo, --session, --worktree or --pr.");
    }
    let n = cards.len();
    let op = if add { BoardOp::Add { id: b.id.clone(), cards: cards.clone() } } else { BoardOp::Remove { id: b.id.clone(), cards: cards.clone() } };
    let now = apply_board(c, op)?.board.unwrap_or(b);
    let what = format!("{n} card{}", if n == 1 { "" } else { "s" });
    if !add {
        return say_done(json, &now, format!("Took {what} off {}.\n", now.name));
    }
    say_unshown(c, json, &now, &cards, format!("Put {what} on {}.\n", now.name))
}

/// What a change to a board did, and which of `cards` won't show on it (`unshown`): `notShown`
/// beside the board in JSON.
fn say_unshown(c: &mut Client, json: bool, now: &BoardDoc, cards: &[String], line: String) -> Result<()> {
    let board = c.board()?;
    let unshown: Vec<Unshown> = cards.iter().filter_map(|card| unshown(board, now, card)).collect();
    if json {
        let mut out = serde_json::to_value(now)?;
        out["notShown"] = serde_json::to_value(&unshown)?;
        return print_json(&out);
    }
    print!("{line}{}", unshown_text(&unshown));
    Ok(())
}

/// A card put on a board by hand that won't show there, and why.
#[derive(Serialize, Debug, PartialEq)]
struct Unshown {
    card: String,
    title: String,
    why: String,
}

/// Why a card put on a board won't show there, if it won't: a board shows only what's in its
/// project, as the app's windows decide it (boardLists.ts `kanbanOf`, canvas/model.ts
/// `entitiesOf`), and keeps what's put on it from elsewhere all the same. Of what the board
/// doesn't know, it says nothing.
fn unshown(board: &Board, doc: &BoardDoc, card: &str) -> Option<Unshown> {
    let (kind, key) = boards::parse_card(card)?;
    let every = doc.project == crate::route::ALL_PROJECTS;
    let place = |project: &str| match project {
        "" => "is in no project".to_string(),
        board::ELSEWHERE => "is outside your projects".to_string(),
        p => format!("is in {p}"),
    };
    let elsewhere = |project: &str| (!every && project != doc.project).then(|| format!("{}, and {} shows what's in {}", place(project), doc.name, doc.project));
    let (title, why) = match kind {
        boards::Kind::Session => all_sessions(board).into_iter().find(|(s, _)| s.key == key).map(|(s, l)| (s.title.clone(), elsewhere(&l.project)))?,
        boards::Kind::Worktree => board.lanes.iter().find(|l| l.id == key).map(|l| (l.callsign.clone(), elsewhere(&l.project)))?,
        boards::Kind::Todo => board.todos.iter().find(|t| t.id == key).map(|t| (t.title.clone(), elsewhere(&t.project)))?,
        boards::Kind::Pr => board.prs.iter().find(|p| p.key == key).map(|p| (format!("#{} {}", p.number, p.title), elsewhere(&p.project)))?,
    };
    let title = if title.trim().is_empty() { "Untitled".to_string() } else { truncate(&title, 60) };
    Some(Unshown { card: card.to_string(), title, why: why? })
}

fn unshown_text(unshown: &[Unshown]) -> String {
    if unshown.is_empty() {
        return String::new();
    }
    let n = if unshown.len() == 1 { "One".to_string() } else { unshown.len().to_string() };
    let lines: String = unshown.iter().map(|u| format!("  {} ({}) {}.\n", u.title, u.card, u.why)).collect();
    format!("{n} won't show there:\n{lines}")
}

/// A todo label by its name, whatever its case, or its id.
fn board_label(store: &TodoStore, name: &str) -> Result<String> {
    let name = name.trim();
    store.labels.iter().find(|l| l.id == name || l.name.eq_ignore_ascii_case(name)).map(|l| l.id.clone()).ok_or_else(|| {
        let names: Vec<&str> = store.labels.iter().map(|l| l.name.as_str()).collect();
        anyhow!("There's no label called {name}.{}", if names.is_empty() { String::new() } else { format!(" The labels are {}.", names.join(", ")) })
    })
}

/// The rule the options say, or none when they say nothing.
fn rule_of(c: &mut Client, a: &RuleArgs) -> Result<Option<Rule>> {
    if a.everything {
        return Ok(Some(Rule::default()));
    }
    let given = !(a.kinds.is_empty() && a.stages.is_empty() && a.statuses.is_empty() && a.labels.is_empty() && a.repos.is_empty() && a.accounts.is_empty() && a.active.is_none() && a.text.as_deref().map_or(true, |t| t.trim().is_empty()));
    if !given {
        return Ok(None);
    }
    let store = store_of(c.board()?);
    let labels = a.labels.iter().map(|n| board_label(&store, n)).collect::<Result<Vec<_>>>()?;
    if let Some(r) = a.repos.iter().find(|r| !r.contains('/')) {
        bail!("A repo is owner/name, not {r}.");
    }
    Ok(Some(Rule {
        kinds: a.kinds.clone(),
        stages: a.stages.clone(),
        statuses: a.statuses.clone(),
        labels,
        repos: a.repos.clone(),
        accounts: a.accounts.clone(),
        active: a.active.clone(),
        text: a.text.clone().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()),
    }))
}

/// What a rule takes in, in words.
fn rule_text(rule: Option<&Rule>, labels: &[Label]) -> String {
    let Some(r) = rule else { return "only what's added".into() };
    let or = |words: Vec<String>| words.join(" or ");
    let mut parts = Vec::new();
    if !r.kinds.is_empty() {
        parts.push(or(r.kinds.iter().map(|k| format!("{}s", k.word()).replace("prs", "PRs")).collect()));
    }
    if !r.labels.is_empty() {
        parts.push(format!("labelled {}", or(r.labels.iter().map(|id| labels.iter().find(|l| &l.id == id).map_or_else(|| id.clone(), |l| l.name.clone())).collect())));
    }
    if !r.stages.is_empty() {
        let word = |s: &boards::Stage| match s {
            boards::Stage::Todo => "not started",
            boards::Stage::Progress => "in progress",
            boards::Stage::Validating => "validating",
            boards::Stage::Review => "in review",
            boards::Stage::Ready => "ready to merge",
            boards::Stage::Done => "done",
        };
        parts.push(or(r.stages.iter().map(|s| word(s).to_string()).collect()));
    }
    if !r.statuses.is_empty() {
        let word = |s: &boards::Status| match s {
            boards::Status::Needs => "needing you",
            boards::Status::Unread => "your turn",
            boards::Status::Working => "working",
        };
        parts.push(or(r.statuses.iter().map(|s| word(s).to_string()).collect()));
    }
    if !r.repos.is_empty() {
        parts.push(format!("in {}", or(r.repos.clone())));
    }
    if !r.accounts.is_empty() {
        parts.push(format!("on {}", or(r.accounts.clone())));
    }
    if let Some(a) = &r.active {
        parts.push(format!("active in the last {a}"));
    }
    if let Some(t) = &r.text {
        parts.push(format!("titled “{t}”"));
    }
    if parts.is_empty() {
        return "everything".into();
    }
    parts.join(", ")
}

fn board_list(c: &mut Client, at: &BoardAt, json: bool) -> Result<()> {
    let project = board_project(c, at)?.ok_or_else(|| anyhow!("Give --project: this folder isn't in a worktree of a project."))?;
    let list = boards::list(&project)?;
    if json {
        return print_json(&list);
    }
    if list.is_empty() {
        println!("{project} has no boards yet. `codealong board new <name>` makes one.");
        return Ok(());
    }
    let labels = store_of(c.board()?).labels;
    // All projects lists every project's boards, so each says its project.
    let every = project == crate::route::ALL_PROJECTS;
    let rows = list
        .iter()
        .map(|b| {
            let mut row = vec![b.id.clone(), b.name.clone(), rule_text(b.rule.as_ref(), &labels), b.added.len().to_string(), b.removed.len().to_string()];
            if every {
                row.insert(2, b.project.clone());
            }
            row
        })
        .collect();
    let heads: &[&str] = if every { &["ID", "NAME", "PROJECT", "TAKES IN", "ADDED", "OFF"] } else { &["ID", "NAME", "TAKES IN", "ADDED", "OFF"] };
    print!("{}", table_text(heads, rows));
    Ok(())
}

fn board_new(c: &mut Client, a: BoardNewArgs, json: bool) -> Result<()> {
    let project = board_project(c, &a.at)?.ok_or_else(|| anyhow!("Give --project: this folder isn't in a worktree of a project."))?;
    let rule = rule_of(c, &a.rule)?;
    let col = |id: &str, name: &str, rule: Option<Rule>, sort: boards::Sort| boards::Column { id: id.into(), name: name.into(), rule, sort, folded: false };
    let columns = match a.columns {
        ColumnsPreset::Stages => None,
        ColumnsPreset::Blank => Some(vec![col("inbox", "Inbox", None, boards::Sort::Hand)]),
        ColumnsPreset::Labels => {
            let store = store_of(c.board()?);
            let named = rule.as_ref().map(|r| r.labels.clone()).unwrap_or_default();
            let labels: Vec<&Label> = if named.is_empty() { store.labels.iter().collect() } else { named.iter().filter_map(|id| store.labels.iter().find(|l| &l.id == id)).collect() };
            if labels.is_empty() {
                bail!("There are no labels to make columns of. Add some with `codealong todo labels add`.");
            }
            let mut cols: Vec<boards::Column> = labels.iter().map(|l| col(&format!("label-{}", l.id), &l.name, Some(Rule { labels: vec![l.id.clone()], ..Default::default() }), boards::Sort::Move)).collect();
            cols.push(col("other", "Other", None, boards::Sort::Hand));
            Some(cols)
        }
    };
    let board = BoardDoc { rule, columns, ..BoardDoc::new(&a.name, &project) };
    let out = apply_board(c, BoardOp::Create { board })?;
    let made = out.board.ok_or_else(|| anyhow!("The board wasn't made."))?;
    if json {
        return print_json(&made);
    }
    let labels = store_of(c.board()?).labels;
    println!("Made the board {} in {}, which takes in {}: {}", made.name, made.project, rule_text(made.rule.as_ref(), &labels), made.id);
    Ok(())
}

fn board_rule(c: &mut Client, a: BoardRuleArgs, json: bool) -> Result<()> {
    let b = find_board(c, Some(&a.board), &a.at)?;
    let rule = match (a.hand_only, rule_of(c, &a.rule)?) {
        (true, _) => None,
        (false, Some(rule)) => Some(rule),
        (false, None) => bail!("Say what it takes in (`codealong board rule --help`), --everything, or --hand-only."),
    };
    let now = apply_board(c, BoardOp::Rule { id: b.id.clone(), rule })?.board.unwrap_or(b);
    let labels = store_of(c.board()?).labels;
    say_done(json, &now, format!("{} now takes in {}.\n", now.name, rule_text(now.rule.as_ref(), &labels)))
}

/// What `board set` reads: a rule and columns, each left as it is when it's not there.
#[derive(serde::Deserialize, Debug, Default)]
struct BoardSpec {
    #[serde(default, deserialize_with = "present")]
    rule: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    columns: Option<Option<Vec<Value>>>,
}

/// A field that's there, null or not.
fn present<'de, T: serde::Deserialize<'de>, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

/// A rule as `board set` takes it: only the parts a rule has, so a misspelled one isn't taken for
/// everything, and labels by name or id.
fn spec_rule(store: &TodoStore, v: &Value) -> Result<Rule> {
    const PARTS: [&str; 8] = ["kinds", "stages", "statuses", "labels", "repos", "accounts", "active", "text"];
    let Some(obj) = v.as_object() else { bail!("A rule is a JSON object, not {v}.") };
    if let Some(k) = obj.keys().find(|k| !PARTS.contains(&k.as_str())) {
        bail!("A rule has no {k}. Its parts are {}.", PARTS.join(", "));
    }
    let mut rule: Rule = serde_json::from_value(v.clone()).context("reading a rule")?;
    rule.labels = rule.labels.iter().map(|n| board_label(store, n)).collect::<Result<_>>()?;
    Ok(rule)
}

fn board_set(c: &mut Client, a: BoardSetArgs, json: bool) -> Result<()> {
    let b = find_board(c, Some(&a.board), &a.at)?;
    let text = read_text(None, Some(a.spec))?.unwrap_or_default();
    let spec: BoardSpec = serde_json::from_str(&text).context("reading the JSON, which is as `codealong board set --help` shows")?;
    if spec.rule.is_none() && spec.columns.is_none() {
        bail!("The JSON has neither a rule nor columns.");
    }
    let store = store_of(c.board()?);
    let rule = spec.rule.map(|r| r.map(|v| spec_rule(&store, &v)).transpose()).transpose()?;
    let columns = spec
        .columns
        .map(|cols| {
            cols.map(|cols| {
                cols.into_iter()
                    .map(|mut v| {
                        let rule = v.get("rule").filter(|r| !r.is_null()).map(|r| spec_rule(&store, r)).transpose()?;
                        if let Some(o) = v.as_object_mut() {
                            // What `board show` lists in a column isn't the column's.
                            o.remove("cards");
                            o.remove("rule");
                        }
                        let mut col: boards::Column = serde_json::from_value(v).context("reading a column, which has an id and a name")?;
                        col.rule = rule;
                        Ok(col)
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()
        })
        .transpose()?;
    let mut now = b.clone();
    if let Some(rule) = rule {
        now = apply_board(c, BoardOp::Rule { id: b.id.clone(), rule })?.board.unwrap_or(now);
    }
    if let Some(columns) = columns {
        now = apply_board(c, BoardOp::Columns { id: b.id.clone(), columns })?.board.unwrap_or(now);
    }
    let labels = store.labels;
    let cols = now.columns.as_ref().map_or_else(|| "the stages".to_string(), |cols| cols.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", "));
    say_done(json, &now, format!("{} takes in {}, in columns {}.\n", now.name, rule_text(now.rule.as_ref(), &labels), cols))
}

/// A board as the main window tells it (src/boards/useBoardAsks.ts).
#[derive(serde::Deserialize, Serialize, Debug, Clone)]
struct BoardName {
    id: String,
    name: String,
    project: String,
}

/// A board's Kanban as the main window tells it.
#[derive(serde::Deserialize, Serialize, Debug)]
struct KanbanOutline {
    board: BoardName,
    rule: Option<Rule>,
    columns: Vec<KanbanOutColumn>,
}

#[derive(serde::Deserialize, Serialize, Debug)]
struct KanbanOutColumn {
    id: String,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rule: Option<Rule>,
    sort: boards::Sort,
    #[serde(default)]
    folded: bool,
    cards: Vec<KanbanOutCard>,
}

#[derive(serde::Deserialize, Serialize, Debug)]
struct KanbanOutCard {
    #[serde(rename = "ref")]
    kind: boards::Kind,
    key: String,
    title: String,
    /// The sessions on a todo, by key.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sessions: Vec<String>,
}

fn board_show(c: &mut Client, a: BoardShowArgs, json: bool) -> Result<()> {
    let b = find_board(c, a.board.as_deref(), &a.at)?;
    let answer = ask_board(c, &b, boards::Ask::Show { layout: a.layout })?;
    match a.layout {
        boards::Layout::Kanban => {
            let out: KanbanOutline = serde_json::from_value(answer)?;
            if json {
                return print_json(&out);
            }
            let labels = store_of(c.board()?).labels;
            print!("{}", kanban_text(&out, &labels));
        }
        boards::Layout::Canvas => {
            let out: CanvasOutline = serde_json::from_value(answer)?;
            if json {
                return print_json(&out);
            }
            print!("{}", outline_text(&out));
        }
    }
    Ok(())
}

fn kanban_text(o: &KanbanOutline, labels: &[Label]) -> String {
    let mut out = format!("{}, a board in {}, takes in {}\n", o.board.name, o.board.project, rule_text(o.rule.as_ref(), labels));
    for col in &o.columns {
        out.push_str(&format!("\n{} ({}{})\n", col.name, col.cards.len(), if col.folded { ", folded" } else { "" }));
        let rows: Vec<Vec<String>> = col
            .cards
            .iter()
            .map(|card| {
                let sessions = match card.sessions.len() {
                    0 => String::new(),
                    n => format!(" ({n} session{})", if n == 1 { "" } else { "s" }),
                };
                vec![format!("  {}", card.kind.word()), format!("{}{sessions}", truncate(&card.title, 60)), card.key.clone()]
            })
            .collect();
        if !rows.is_empty() {
            out.push_str(&table_text(&["", "", ""], rows).lines().skip(1).map(|l| format!("{}\n", l.trim_end())).collect::<String>());
        }
    }
    out
}

/// What the main window says a move did.
#[derive(serde::Deserialize, Serialize, Debug)]
struct Moved {
    /// What it did, or would do, in words.
    said: String,
    /// The column the card is in now, or would be.
    column: String,
    done: bool,
}

fn board_move(c: &mut Client, a: BoardMoveArgs, json: bool) -> Result<()> {
    let b = find_board(c, Some(&a.board), &a.at)?;
    let cards = card_ids(c, &a.card)?;
    let [card] = cards.as_slice() else { bail!("Name one card to move: one --todo, --session, --worktree or --pr.") };
    let out: Moved = serde_json::from_value(ask_board(c, &b, boards::Ask::Move { card: card.clone(), to: a.to, before: a.before, dry_run: a.dry_run })?)?;
    if json {
        return print_json(&out);
    }
    println!("{}", out.said);
    Ok(())
}

// ---- a board's canvas ----

/// The plan `board arrange` reads.
#[derive(serde::Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct CanvasPlan {
    frames: Vec<PlanFrame>,
}

/// A frame of the plan, with its cards by kind as an agent names them.
#[derive(serde::Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
struct PlanFrame {
    title: String,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    worktrees: Vec<String>,
    #[serde(default)]
    sessions: Vec<String>,
    #[serde(default)]
    todos: Vec<String>,
    #[serde(default)]
    prs: Vec<String>,
}

/// What the main window says it arranged (canvas/model.ts `arrange`).
#[derive(serde::Deserialize, Serialize, Debug)]
struct CanvasArranged {
    board: BoardName,
    frames: Vec<ArrangedFrame>,
    missing: Vec<boards::Card>,
    rest: usize,
}

#[derive(serde::Deserialize, Serialize, Debug)]
struct ArrangedFrame {
    id: String,
    title: String,
    cards: usize,
}

/// What a board's canvas holds, as the main window tells it (canvas/model.ts `outline`).
#[derive(serde::Deserialize, Serialize, Debug)]
struct CanvasOutline {
    board: BoardName,
    exists: bool,
    frames: Vec<OutlineFrame>,
    cards: Vec<OutlineCard>,
    notes: Vec<OutlineNote>,
}

#[derive(serde::Deserialize, Serialize, Debug)]
struct OutlineFrame {
    id: String,
    title: String,
    color: String,
    by: String,
    frame: Option<String>,
}

#[derive(serde::Deserialize, Serialize, Debug)]
struct OutlineCard {
    #[serde(rename = "ref")]
    kind: boards::Kind,
    key: String,
    title: Option<String>,
    frame: Option<String>,
    gone: bool,
}

#[derive(serde::Deserialize, Serialize, Debug)]
struct OutlineNote {
    id: String,
    text: String,
    frame: Option<String>,
    by: String,
}

fn outline_text(o: &CanvasOutline) -> String {
    if !o.exists {
        return format!("The {} canvas was never shown: Codealong makes it, tidied, when it's first shown or arranged.\n", o.board.name);
    }
    let mut out = format!("The {} canvas\n", o.board.name);
    let groups = o.frames.iter().map(Some).chain(std::iter::once(None));
    for frame in groups {
        let id = frame.map(|f| f.id.as_str());
        let cards: Vec<&OutlineCard> = o.cards.iter().filter(|c| c.frame.as_deref() == id).collect();
        let notes: Vec<&OutlineNote> = o.notes.iter().filter(|n| n.frame.as_deref() == id).collect();
        if frame.is_none() && cards.is_empty() && notes.is_empty() {
            continue;
        }
        out.push('\n');
        match frame {
            Some(f) => out.push_str(&format!("{} ({})\n", f.title, match f.by.as_str() {
                "tidy" => "a tidy made it",
                "arrange" => "arranged",
                _ => "drawn",
            })),
            None => out.push_str("In no frame\n"),
        }
        for n in notes {
            out.push_str(&format!("  Note: {}\n", truncate(&n.text.split_whitespace().collect::<Vec<_>>().join(" "), 100)));
        }
        let rows: Vec<Vec<String>> = cards
            .iter()
            .map(|c| {
                let title = truncate(c.title.as_deref().unwrap_or("Untitled"), 60);
                vec![format!("  {}", c.kind.word()), if c.gone { format!("{title} (gone)") } else { title }, c.key.clone()]
            })
            .collect();
        if !rows.is_empty() {
            // Columns under the frame's name, without a heading row.
            out.push_str(&table_text(&["", "", ""], rows).lines().skip(1).map(|l| format!("{}\n", l.trim_end())).collect::<String>());
        }
    }
    out
}

/// The plan's frames as the app takes them, each card named by its key on the board. Every
/// card that can't be named is said at once, so a plan is fixed in one go.
fn plan_frames(c: &mut Client, plan: CanvasPlan) -> Result<Vec<boards::Frame>> {
    if plan.frames.is_empty() {
        bail!("The plan has no frames.");
    }
    let todos = store_of(c.board()?);
    let mut wrong = Vec::new();
    let mut frames = Vec::new();
    for f in plan.frames {
        if f.title.trim().is_empty() {
            wrong.push("A frame has no title.".to_string());
        }
        if let Some(color) = f.color.as_deref().filter(|col| !LABEL_COLORS.contains(col)) {
            wrong.push(format!("{color} isn't a color. Give one of {}.", LABEL_COLORS.join(", ")));
        }
        let mut cards = Vec::new();
        let mut add = |kind: boards::Kind, key: Result<String>| match key {
            Ok(key) => cards.push(boards::Card { kind, key }),
            Err(e) => wrong.push(e.to_string()),
        };
        for w in &f.worktrees {
            add(boards::Kind::Worktree, resolve_worktree(c, w));
        }
        for s in &f.sessions {
            add(boards::Kind::Session, resolve_session(c, s));
        }
        for t in &f.todos {
            add(boards::Kind::Todo, resolve_todo(&todos, t));
        }
        for p in &f.prs {
            add(boards::Kind::Pr, board_pr(c, p));
        }
        if cards.is_empty() && f.note.as_deref().map_or(true, |n| n.trim().is_empty()) {
            wrong.push(format!("The frame {} has no cards and no note.", f.title.trim()));
        }
        frames.push(boards::Frame { title: f.title.trim().to_string(), color: f.color, note: f.note, cards });
    }
    if !wrong.is_empty() {
        bail!("{}", wrong.join("\n"));
    }
    Ok(frames)
}

fn board_arrange(c: &mut Client, a: BoardArrangeArgs, json: bool) -> Result<()> {
    let b = find_board(c, a.board.as_deref(), &a.at)?;
    let text = read_text(None, Some(a.plan))?.unwrap_or_default();
    let plan: CanvasPlan = serde_json::from_str(&text).context("reading the plan, which is JSON as `codealong board arrange --help` shows")?;
    let frames = plan_frames(c, plan)?;
    let out: CanvasArranged = serde_json::from_value(ask_board(c, &b, boards::Ask::Arrange { frames, rest: a.rest })?)?;
    if a.show {
        let focus: Vec<&str> = out.frames.iter().map(|f| f.id.as_str()).collect();
        c.show(json!({ "kind": "board", "board": out.board.id, "project": out.board.project, "layout": "canvas", "focus": focus }))?;
    }
    if json {
        return print_json(&out);
    }
    print!("{}", arranged_text(&out, a.rest));
    Ok(())
}

fn arranged_text(a: &CanvasArranged, rest: boards::Rest) -> String {
    let cards = |n: usize| format!("{n} card{}", if n == 1 { "" } else { "s" });
    let frames: Vec<String> = a.frames.iter().map(|f| format!("{} ({})", f.title, cards(f.cards))).collect();
    let name = &a.board.name;
    let mut out = match frames.len() {
        0 => format!("Made no frames on the {name} canvas: nothing in the plan is on its board.\n"),
        n => format!("Arranged the {name} canvas in {n} frame{}: {}.\n", if n == 1 { "" } else { "s" }, frames.join(", ")),
    };
    if a.rest > 0 {
        out.push_str(&match rest {
            boards::Rest::Keep => format!("{} no frame named went below them, by worktree.\n", cards(a.rest)),
            boards::Rest::Off => format!("{} no frame named came off the canvas and the board.\n", cards(a.rest)),
        });
    }
    if !a.missing.is_empty() {
        let missing: Vec<String> = a.missing.iter().map(|m| format!("{} {}", m.kind.word(), m.key)).collect();
        out.push_str(&format!("Left out, as {} doesn't have them: {}.\n", a.board.project, missing.join(", ")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::tests::{session, todo};
    use crate::fixtures::lane;
    use crate::model::Lane;
    use crate::todos::{Todo, TodoStatus};

    #[test]
    fn board_commands_are_turned_down_until_boards_are_turned_on() {
        let err = boards_on(&Config::default()).unwrap_err().to_string();
        assert!(err.contains("Settings › Experimental") && err.contains("[experimental]"), "{err}");
        let on = Config { experimental: crate::config::ExperimentalCfg { boards: true, ..Default::default() }, ..Config::default() };
        assert!(boards_on(&on).is_ok());
    }

    #[test]
    fn a_board_name_two_projects_share_is_turned_down_under_all_projects() {
        let all = crate::route::ALL_PROJECTS;
        let board = |id: &str, name: &str, project: &str| BoardDoc { id: id.into(), ..BoardDoc::new(name, project) };
        let stored = vec![board("b-1", "Release", "Atlas"), board("b-2", "Release", "Kestrel"), board("b-3", "Ideas", all), board("b-4", "Site", "Atlas")];
        // As boards::list gives them: under All projects, every project's.
        let listed = |p: &str| stored.iter().filter(|b| b.project == p || p == all).cloned().collect::<Vec<_>>();
        let named = |name: &str, p: Option<&str>| board_named(name, p, p.map(listed).unwrap_or_default(), stored.clone()).map(|b| b.id);
        assert_eq!(named("release", Some("Kestrel")).unwrap(), "b-2");
        let err = named("Release", Some(all)).unwrap_err().to_string();
        assert!(err.starts_with("2 projects have a board called Release"), "{err}");
        // A name only one project has is that board, whichever project's asked.
        assert_eq!(named("Site", Some(all)).unwrap(), "b-4");
        assert_eq!(named("Site", Some("Kestrel")).unwrap(), "b-4");
        assert_eq!(named("Ideas", Some(all)).unwrap(), "b-3");
        // A project's name names no board of its own.
        assert!(named("Kestrel", None).unwrap_err().to_string().starts_with("There's no board Kestrel"));
        // Named none, a command is about the one board its project has.
        let only = |p: &str| only_board(p, listed(p)).map(|b| b.id);
        assert_eq!(only("Kestrel").unwrap(), "b-2");
        assert_eq!(only(all).unwrap(), "b-3");
        assert!(only("Atlas").unwrap_err().to_string().starts_with("Atlas has 2 boards: name one (Release, Site)"));
        assert!(only("Northwind").unwrap_err().to_string().starts_with("Northwind has no boards."));
    }

    #[test]
    fn board_add_says_which_cards_wont_show_as_they_are_outside_the_boards_project() {
        let at = |id: &str, project: &str, hidden: bool, key: &str| Lane { project: project.into(), hidden, ..lane(id, vec![session(key, true, None)]) };
        let lanes = vec![at("/r/app", "Northwind", false, "app"), at("/r/chats", board::ELSEWHERE, false, "chat"), at("/r/notes", board::ELSEWHERE, true, "note"), at("/r/site", "Atlas", false, "site")];
        let todos = vec![Todo { project: "Northwind".into(), ..todo("t-here", TodoStatus::Done, &[], &[]) }, Todo { project: "Atlas".into(), ..todo("t-there", TodoStatus::Todo, &[], &[]) }, todo("t-none", TodoStatus::Todo, &[], &[])];
        let b = Board { lanes, todos, ..Board::default() };
        let release = BoardDoc::new("Release", "Northwind");
        let why = |doc: &BoardDoc, card: &str| unshown(&b, doc, card).map(|u| u.why);
        // What's in the project shows, archived, a helper or done; what the board doesn't know, it can't say.
        for card in ["session:app", "worktree:/r/app", "todo:t-here", "session:unknown"] {
            assert_eq!(why(&release, card), None, "{card}");
        }
        assert_eq!(why(&release, "session:site").as_deref(), Some("is in Atlas, and Release shows what's in Northwind"));
        assert_eq!(why(&release, "todo:t-there").as_deref(), Some("is in Atlas, and Release shows what's in Northwind"));
        assert_eq!(why(&release, "session:chat").as_deref(), Some("is outside your projects, and Release shows what's in Northwind"));
        assert_eq!(why(&release, "todo:t-none").as_deref(), Some("is in no project, and Release shows what's in Northwind"));
        // A board of All projects shows what's in every project, Elsewhere's out-of-sight worktrees too.
        let every = BoardDoc::new("Everything", crate::route::ALL_PROJECTS);
        for card in ["session:site", "session:chat", "session:note", "worktree:/r/notes", "todo:t-there", "todo:t-none"] {
            assert_eq!(why(&every, card), None, "{card}");
        }
        let said = unshown_text(&[unshown(&b, &release, "session:site").unwrap()]);
        assert_eq!(said, "One won't show there:\n  Title site (session:site) is in Atlas, and Release shows what's in Northwind.\n");
        assert_eq!(unshown_text(&[]), "");
    }

    #[test]
    fn a_canvas_plan_names_each_card_by_its_board_key_and_says_every_one_it_cannot() {
        let mut b = Board::default();
        b.lanes = vec![lane("/work/a", vec![crate::fixtures::session("claude:claude:abc123")])];
        b.todos = vec![todo("t-abc", TodoStatus::Todo, &[], &[])];
        let mut c = Client { app: None, verbose: false, cfg: None, board: Some(b), from_app: false, started: false, said: false, me: None, elsewhere: None, veiled: None };
        let plan = |v: Value| serde_json::from_value::<CanvasPlan>(v);
        let card = |kind, key: &str| boards::Card { kind, key: key.into() };
        let good = plan(json!({ "frames": [{ "title": " Billing ", "color": "teal", "note": "Invoices", "prs": ["https://github.com/acme/app/pull/7"], "todos": ["t-a"], "sessions": ["claude:claude:abc"], "worktrees": ["/work/a"] }] })).unwrap();
        assert_eq!(
            plan_frames(&mut c, good).unwrap(),
            vec![boards::Frame {
                title: "Billing".into(),
                color: Some("teal".into()),
                note: Some("Invoices".into()),
                // Worktrees first, so each heads its column in the order given.
                cards: vec![card(boards::Kind::Worktree, "/work/a"), card(boards::Kind::Session, "claude:claude:abc123"), card(boards::Kind::Todo, "t-abc"), card(boards::Kind::Pr, "acme/app#7")],
            }]
        );
        let bad = plan(json!({ "frames": [{ "title": "", "color": "mauve", "sessions": ["nope"] }, { "title": "Note only", "note": "Fine" }] })).unwrap();
        let said = plan_frames(&mut c, bad).unwrap_err().to_string();
        for part in ["A frame has no title.", "mauve isn't a color", "There's no session nope", "The frame  has no cards and no note."] {
            assert!(said.contains(part), "{part} in {said}");
        }
        assert!(!said.contains("Note only"));
        // A misspelled field is an error, not a frame with nothing in it.
        assert!(plan(json!({ "frames": [{ "title": "x", "session": ["a"] }] })).is_err());
        assert!(plan_frames(&mut c, plan(json!({ "frames": [] })).unwrap()).is_err());
    }

    #[test]
    fn a_canvas_reads_frame_by_frame() {
        let out: CanvasOutline = serde_json::from_value(json!({
            "board": { "id": "b-1", "name": "Acme", "project": "Acme" }, "exists": true,
            "frames": [{ "id": "arrange:1:0", "title": "Billing", "color": "teal", "by": "arrange", "frame": null }, { "id": "auto:repo:app", "title": "app", "color": "blue", "by": "tidy", "frame": null }],
            "cards": [
                { "ref": "session", "key": "claude:claude:abc", "title": "Fix invoices", "frame": "arrange:1:0", "gone": false },
                { "ref": "worktree", "key": "/work/a", "title": "a", "frame": "auto:repo:app", "gone": false },
                { "ref": "session", "key": "claude:claude:old", "title": "Old", "frame": null, "gone": true }
            ],
            "notes": [{ "id": "arrange:1:0:note", "text": "Two\nsessions", "frame": "arrange:1:0", "by": "arrange" }]
        }))
        .unwrap();
        assert_eq!(
            outline_text(&out),
            "The Acme canvas\n\nBilling (arranged)\n  Note: Two sessions\n  session  Fix invoices  claude:claude:abc\n\napp (a tidy made it)\n  worktree  a  /work/a\n\nIn no frame\n  session  Old (gone)  claude:claude:old\n"
        );
        assert!(outline_text(&CanvasOutline { exists: false, frames: vec![], cards: vec![], notes: vec![], ..out }).starts_with("The Acme canvas was never shown"));
    }

    #[test]
    fn an_arrangement_says_what_went_where() {
        let a = CanvasArranged {
            board: BoardName { id: "b-1".into(), name: "Acme".into(), project: "Acme".into() },
            frames: vec![ArrangedFrame { id: "arrange:1:0".into(), title: "Billing".into(), cards: 3 }, ArrangedFrame { id: "arrange:1:1".into(), title: "Site".into(), cards: 1 }],
            missing: vec![boards::Card { kind: boards::Kind::Session, key: "claude:claude:gone".into() }],
            rest: 1,
        };
        assert_eq!(
            arranged_text(&a, boards::Rest::Keep),
            "Arranged the Acme canvas in 2 frames: Billing (3 cards), Site (1 card).\n1 card no frame named went below them, by worktree.\nLeft out, as Acme doesn't have them: session claude:claude:gone.\n"
        );
        assert!(arranged_text(&a, boards::Rest::Off).contains("1 card no frame named came off the canvas and the board."));
    }

    #[test]
    fn a_rule_says_what_it_takes_in() {
        let labels = vec![Label { id: "l-1".into(), name: "release".into(), color: "blue".into() }];
        assert_eq!(rule_text(None, &labels), "only what's added");
        assert_eq!(rule_text(Some(&Rule::default()), &labels), "everything");
        let rule = Rule { kinds: vec![boards::Kind::Todo, boards::Kind::Pr], labels: vec!["l-1".into(), "l-gone".into()], stages: vec![boards::Stage::Review], statuses: vec![boards::Status::Needs], repos: vec!["acme/app".into()], accounts: vec!["codex".into()], active: Some("7d".into()), text: Some("login".into()) };
        assert_eq!(rule_text(Some(&rule), &labels), "todos or PRs, labelled release or l-gone, in review, needing you, in acme/app, on codex, active in the last 7d, titled “login”");
    }

    #[test]
    fn a_rule_from_json_has_only_a_rules_parts_and_names_labels_either_way() {
        let store = TodoStore { todos: vec![], labels: vec![Label { id: "l-1".into(), name: "Release".into(), color: "blue".into() }] };
        let rule = spec_rule(&store, &json!({ "labels": ["release", "l-1"], "stages": ["done"] })).unwrap();
        assert_eq!(rule.labels, ["l-1", "l-1"]);
        assert_eq!(rule.stages, [boards::Stage::Done]);
        assert!(spec_rule(&store, &json!({ "label": ["release"] })).unwrap_err().to_string().contains("A rule has no label"));
        assert!(spec_rule(&store, &json!({ "labels": ["nope"] })).is_err());
        assert!(spec_rule(&store, &json!(["todo"])).is_err());
        // `board set` leaves out what isn't there, and clears what's null.
        let spec: BoardSpec = serde_json::from_value(json!({ "rule": null })).unwrap();
        assert_eq!((spec.rule, spec.columns.is_none()), (Some(None), true));
    }

    #[test]
    fn a_kanban_reads_column_by_column() {
        let out: KanbanOutline = serde_json::from_value(json!({
            "board": { "id": "b-1", "name": "Release", "project": "Acme" },
            "rule": { "labels": ["l-1"] },
            "columns": [
                { "id": "todo", "name": "To do", "rule": { "stages": ["todo"] }, "sort": "hand", "cards": [{ "ref": "todo", "key": "t-1", "title": "Fix login", "sessions": ["claude:claude:abc"] }] },
                { "id": "done", "name": "Done", "sort": "latest", "folded": true, "cards": [] }
            ]
        }))
        .unwrap();
        let labels = vec![Label { id: "l-1".into(), name: "release".into(), color: "blue".into() }];
        assert_eq!(kanban_text(&out, &labels), "Release, a board in Acme, takes in labelled release\n\nTo do (1)\n  todo  Fix login (1 session)  t-1\n\nDone (0, folded)\n");
    }
}
