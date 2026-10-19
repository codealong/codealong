//! Boards: named sets of work in a project, each shown as Kanban or as Canvas (src/boards/). A
//! board holds what its rule takes in and what you added by hand, less what you took off; its
//! Kanban columns, where you put cards by hand and in what order; and its canvas, where each card
//! sits among your notes and frames. Every project has a board of its own, named after it, whose
//! rule takes in everything in the project: the Kanban and the Canvas as they were before there
//! were boards. It's written to the file the first time it changes.
//!
//! Kept in `boards.json` next to the config. The engine owns the file and makes every change
//! through `apply`, so the app and the `codealong` command never write over each other. What the
//! rules and columns mean, and the canvas's shape, are the window's (src/boards/, src/canvas/):
//! the engine keeps a canvas as it's given, so a newer window's fields survive an older engine.
//! `codealong board show|move|arrange` asks the main window, which works a board out (`ask`).

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::config::support_dir;
use crate::store;

pub fn path() -> PathBuf {
    support_dir().join("boards.json")
}

/// Where each project's canvas was kept before there were boards. Read into boards.json until
/// that's first written, and left as it was.
fn canvases_path() -> PathBuf {
    support_dir().join("canvas.json")
}

const MAX_NAME: usize = 100;
const PROJECT_PREFIX: &str = "p:";

/// The id of a project's own board.
pub fn project_board_id(project: &str) -> String {
    format!("{PROJECT_PREFIX}{project}")
}

/// What a card shows.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Worktree,
    Session,
    Todo,
    Pr,
}

impl Kind {
    pub fn word(self) -> &'static str {
        match self {
            Kind::Worktree => "worktree",
            Kind::Session => "session",
            Kind::Todo => "todo",
            Kind::Pr => "pr",
        }
    }
}

/// A card as boards name it, `<kind>:<key>`: a worktree's folder, a session's key, a todo's id,
/// a PR's `owner/name#number` (canvas/model.ts `refId`).
pub fn card_id(kind: Kind, key: &str) -> String {
    format!("{}:{key}", kind.word())
}

pub fn parse_card(id: &str) -> Option<(Kind, &str)> {
    let (kind, key) = id.split_once(':')?;
    let kind = [Kind::Worktree, Kind::Session, Kind::Todo, Kind::Pr].into_iter().find(|k| k.word() == kind)?;
    (!key.is_empty()).then_some((kind, key))
}

/// How far a card's work has got, as the stage columns have it (boardLists.ts `todoStage`,
/// `sessionStage`).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Todo,
    Progress,
    Review,
    Done,
}

/// An agent's status, as the sidebar's statuses name them: Needs you, Your turn, Working.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Needs,
    Unread,
    Working,
}

/// How recently a card's work moved on.
pub const ACTIVE: [&str; 4] = ["1d", "3d", "7d", "30d"];

/// What a rule takes in (src/boards/rules.ts `matches`). Each part it has must hold, and a part
/// holds when the card has any of what it lists; a rule with no parts takes in everything.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<Kind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages: Vec<Stage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub statuses: Vec<Status>,
    /// Todo labels, by id: todos that have one, and the sessions and worktrees on them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    /// Repos by GitHub slug (`owner/name`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repos: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accounts: Vec<String>,
    /// One of `ACTIVE`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
    /// Words its title has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// How a Kanban column orders its cards.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Sort {
    /// Yours, as you drag them.
    #[default]
    Hand,
    /// Whose move it is: what needs you first.
    Move,
    /// The latest first.
    Latest,
}

/// A Kanban column. A card goes in the first column whose rule takes it, else in the column you
/// put it in, else in the first column without a rule (src/boards/rules.ts `columnOf`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<Rule>,
    #[serde(default)]
    pub sort: Sort,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub folded: bool,
}

/// A board as the file keeps it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BoardDoc {
    pub id: String,
    pub name: String,
    /// The project it's in, as the app names it.
    pub project: String,
    /// What comes onto it by itself: none for only what's added by hand.
    #[serde(default)]
    pub rule: Option<Rule>,
    /// Cards added by hand, and taken off by hand (`card_id`).
    #[serde(default)]
    pub added: Vec<String>,
    #[serde(default)]
    pub removed: Vec<String>,
    /// When each card was added by hand: a canvas brings on by itself only what was added after
    /// its last tidy or clear (canvas/model.ts `recentRefs`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub added_at: BTreeMap<String, i64>,
    /// Its Kanban columns; none for the four stages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<Column>>,
    /// The column each card was put in by hand, by card.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub placed: BTreeMap<String, String>,
    /// Cards in the order you put them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<String>,
    /// Its canvas, in the window's shape (canvas/model.ts `CanvasDoc`); null until it's first shown.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub canvas: Value,
    #[serde(default)]
    pub created_at: i64,
    /// What a newer build wrote that this one doesn't know.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl BoardDoc {
    /// A project's own board, as it is until it first changes: everything in the project.
    pub fn of_project(project: &str) -> BoardDoc {
        BoardDoc {
            id: project_board_id(project),
            name: project.to_string(),
            project: project.to_string(),
            rule: Some(Rule::default()),
            added: vec![],
            removed: vec![],
            added_at: BTreeMap::new(),
            columns: None,
            placed: BTreeMap::new(),
            order: vec![],
            canvas: Value::Null,
            created_at: 0,
            extra: Map::new(),
        }
    }

    pub fn is_projects(&self) -> bool {
        self.id == project_board_id(&self.project)
    }

    /// Without its canvas, which only the canvas reads.
    fn listed(mut self) -> BoardDoc {
        self.canvas = Value::Null;
        self
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct BoardsFile {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub boards: Vec<BoardDoc>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A change to a board. Each names its board by id; a project's own board is made as it first
/// changes.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum BoardOp {
    /// A new board. An id that's empty or taken is replaced.
    Create { board: BoardDoc },
    Rename { id: String, name: String },
    Delete { id: String },
    /// Puts cards on the board by hand, back on it if they were taken off.
    Add { id: String, cards: Vec<String> },
    /// Takes cards off the board, and keeps them off.
    Remove { id: String, cards: Vec<String> },
    Rule { id: String, rule: Option<Rule> },
    /// Its Kanban columns; none for the four stages.
    Columns { id: String, columns: Option<Vec<Column>> },
    /// Puts a card in a column by hand, or none to leave it to the rules.
    Place {
        id: String,
        card: String,
        #[serde(default)]
        column: Option<String>,
    },
    /// The order of cards you put them in.
    Order { id: String, order: Vec<String> },
}

impl BoardOp {
    pub fn board(&self) -> &str {
        match self {
            BoardOp::Create { board } => &board.id,
            BoardOp::Rename { id, .. } | BoardOp::Delete { id } | BoardOp::Add { id, .. } | BoardOp::Remove { id, .. } | BoardOp::Rule { id, .. } | BoardOp::Columns { id, .. } | BoardOp::Place { id, .. } | BoardOp::Order { id, .. } => id,
        }
    }
}

/// What a change did: the board as it is now, or none once it's deleted, and its project.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub board: Option<BoardDoc>,
    pub id: String,
    pub project: String,
}

/// The file as it is, or for none yet, the canvases kept before there were boards.
fn read() -> Result<BoardsFile> {
    if let Some(file) = store::read::<BoardsFile>(&path(), "boards")? {
        return Ok(file);
    }
    // A canvas file that doesn't read gives no boards, and is left as it is.
    Ok(from_canvases(&store::text(&canvases_path()).ok().flatten().unwrap_or_default()))
}

/// Each project's canvas from canvas.json, as its own board's canvas. What was taken off a canvas
/// is taken off its board.
fn from_canvases(text: &str) -> BoardsFile {
    let mut file = BoardsFile { version: 1, ..Default::default() };
    let Ok(Value::Object(root)) = serde_json::from_str::<Value>(text) else { return file };
    let Some(Value::Object(canvases)) = root.get("canvases") else { return file };
    for (project, doc) in canvases {
        let mut doc = doc.clone();
        let mut board = BoardDoc::of_project(project);
        if let Some(Value::Array(hidden)) = doc.as_object_mut().and_then(|d| d.remove("hidden")) {
            board.removed = hidden.iter().filter_map(Value::as_str).filter(|id| parse_card(id).is_some()).map(str::to_string).collect();
        }
        board.canvas = doc;
        file.boards.push(board);
    }
    file
}

/// Reads the file under its lock, lets `f` change it, and saves it when `f` says it changed.
fn change<R>(f: impl FnOnce(&mut BoardsFile) -> Result<(bool, R)>) -> Result<R> {
    let _lock = store::lock(&path())?;
    let mut file = read()?;
    let (changed, out) = f(&mut file)?;
    if changed {
        file.version = 1;
        store::save(&path(), &file)?;
    }
    Ok(out)
}

/// A project's boards, its own first and then the rest as they were made, without their canvases.
pub fn list(project: &str) -> Result<Vec<BoardDoc>> {
    Ok(listed(&read()?, project))
}

fn listed(file: &BoardsFile, project: &str) -> Vec<BoardDoc> {
    let own = project_board_id(project);
    let mut out = vec![file.boards.iter().find(|b| b.id == own).cloned().unwrap_or_else(|| BoardDoc::of_project(project)).listed()];
    let mut rest: Vec<&BoardDoc> = file.boards.iter().filter(|b| b.project == project && b.id != own).collect();
    rest.sort_by_key(|b| b.created_at);
    out.extend(rest.into_iter().map(|b| b.clone().listed()));
    out
}

/// Every board the file has, without their canvases: for naming one from the command.
pub fn all() -> Result<Vec<BoardDoc>> {
    Ok(read()?.boards.into_iter().map(BoardDoc::listed).collect())
}

/// A board's canvas, or null when it has none yet.
pub fn canvas(id: &str) -> Result<Value> {
    Ok(read()?.boards.into_iter().find(|b| b.id == id).map(|b| b.canvas).unwrap_or(Value::Null))
}

/// Saves a board's canvas.
pub fn save_canvas(id: &str, doc: Value) -> Result<()> {
    if !doc.is_object() {
        bail!("A canvas is a JSON object.");
    }
    change(|file| {
        board_mut(file, id)?.canvas = doc;
        Ok((true, ()))
    })
}

/// Makes a change to a board.
pub fn apply(op: BoardOp, now: i64) -> Result<Applied> {
    change(|file| apply_to(file, op, now, new_id))
}

fn new_id(now: i64) -> String {
    let mut b = [0u8; 4];
    crate::os::system::random_bytes(&mut b);
    format!("b-{}{}", base36(now as u64), base36(u32::from_le_bytes(b) as u64 % 36u64.pow(4)))
}

fn base36(mut n: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    loop {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
        if n == 0 {
            break;
        }
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// The board `id`, made first when it's a project's own board that isn't in the file yet.
fn board_mut<'a>(file: &'a mut BoardsFile, id: &str) -> Result<&'a mut BoardDoc> {
    if !file.boards.iter().any(|b| b.id == id) {
        let Some(project) = id.strip_prefix(PROJECT_PREFIX).filter(|p| !p.is_empty()) else { bail!("There's no board {id}.") };
        file.boards.push(BoardDoc::of_project(project));
    }
    Ok(file.boards.iter_mut().find(|b| b.id == id).expect("just made"))
}

fn clean_name(name: &str) -> Result<String> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        bail!("A board needs a name.");
    }
    Ok(name.chars().take(MAX_NAME).collect())
}

/// Turns down a name another board in the project has, whatever its case: boards are named by it.
fn name_free(file: &BoardsFile, project: &str, name: &str, but: &str) -> Result<()> {
    let taken = |b: &BoardDoc| b.project == project && b.id != but && b.name.to_lowercase() == name.to_lowercase();
    // A project's own board is there whether or not the file has it yet.
    if file.boards.iter().any(taken) || (but != project_board_id(project) && project.to_lowercase() == name.to_lowercase() && !file.boards.iter().any(|b| b.id == project_board_id(project))) {
        bail!("{project} already has a board called {name}.");
    }
    Ok(())
}

fn cards(given: &[String]) -> Result<Vec<String>> {
    let wrong: Vec<&str> = given.iter().map(String::as_str).filter(|c| parse_card(c).is_none()).collect();
    if !wrong.is_empty() {
        bail!("Not cards: {}. A card is worktree:<folder>, session:<key>, todo:<id> or pr:<owner/name#number>.", wrong.join(", "));
    }
    Ok(given.to_vec())
}

fn check_columns(columns: &[Column]) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for c in columns {
        if c.id.trim().is_empty() || c.name.trim().is_empty() {
            bail!("Every column needs an id and a name.");
        }
        if !seen.insert(c.id.as_str()) {
            bail!("Two columns have the id {}.", c.id);
        }
        check_rule(c.rule.as_ref())?;
    }
    Ok(())
}

fn check_rule(rule: Option<&Rule>) -> Result<()> {
    if let Some(active) = rule.and_then(|r| r.active.as_deref()) {
        if !ACTIVE.contains(&active) {
            bail!("Active is one of {}, not {active}.", ACTIVE.join(", "));
        }
    }
    Ok(())
}

fn apply_to(file: &mut BoardsFile, op: BoardOp, now: i64, new_id: impl Fn(i64) -> String) -> Result<(bool, Applied)> {
    let id = op.board().to_string();
    let changed = match op {
        BoardOp::Create { mut board } => {
            board.name = clean_name(&board.name)?;
            if board.project.trim().is_empty() {
                bail!("A board needs a project.");
            }
            check_rule(board.rule.as_ref())?;
            if let Some(columns) = &board.columns {
                check_columns(columns)?;
            }
            name_free(file, &board.project, &board.name, "")?;
            if board.id.is_empty() || board.id.starts_with(PROJECT_PREFIX) || file.boards.iter().any(|b| b.id == board.id) {
                board.id = new_id(now);
            }
            board.added = cards(&board.added)?;
            board.removed = cards(&board.removed)?;
            board.created_at = now;
            let out = Applied { board: Some(board.clone().listed()), id: board.id.clone(), project: board.project.clone() };
            file.boards.push(board);
            return Ok((true, out));
        }
        BoardOp::Delete { id } => {
            let Some(at) = file.boards.iter().position(|b| b.id == id) else {
                if id.starts_with(PROJECT_PREFIX) {
                    bail!("A project's own board can't be deleted.");
                }
                bail!("There's no board {id}.");
            };
            if file.boards[at].is_projects() {
                bail!("A project's own board can't be deleted.");
            }
            let gone = file.boards.remove(at);
            return Ok((true, Applied { board: None, id, project: gone.project }));
        }
        BoardOp::Rename { id, name } => {
            let name = clean_name(&name)?;
            let project = board_mut(file, &id)?.project.clone();
            name_free(file, &project, &name, &id)?;
            let b = board_mut(file, &id)?;
            let changed = b.name != name;
            b.name = name;
            changed
        }
        BoardOp::Add { id, cards: given } => {
            let given = cards(&given)?;
            let b = board_mut(file, &id)?;
            let before = (b.added.clone(), b.removed.clone());
            b.removed.retain(|c| !given.contains(c));
            for c in given {
                if !b.added.contains(&c) {
                    b.added_at.insert(c.clone(), now);
                    b.added.push(c);
                }
            }
            (b.added.clone(), b.removed.clone()) != before
        }
        BoardOp::Remove { id, cards: given } => {
            let given = cards(&given)?;
            let b = board_mut(file, &id)?;
            let before = (b.added.clone(), b.removed.clone(), b.placed.clone(), b.order.clone());
            b.added.retain(|c| !given.contains(c));
            b.added_at.retain(|c, _| !given.contains(c));
            b.placed.retain(|c, _| !given.contains(c));
            b.order.retain(|c| !given.contains(c));
            for c in given {
                if !b.removed.contains(&c) {
                    b.removed.push(c);
                }
            }
            (b.added.clone(), b.removed.clone(), b.placed.clone(), b.order.clone()) != before
        }
        BoardOp::Rule { id, rule } => {
            check_rule(rule.as_ref())?;
            let b = board_mut(file, &id)?;
            let changed = b.rule != rule;
            b.rule = rule;
            changed
        }
        BoardOp::Columns { id, columns } => {
            if let Some(columns) = &columns {
                check_columns(columns)?;
            }
            let b = board_mut(file, &id)?;
            let changed = b.columns != columns;
            // What was put in a column that's gone goes back to the rules.
            let ids: Vec<&str> = columns.iter().flatten().map(|c| c.id.as_str()).collect();
            b.placed.retain(|_, col| ids.contains(&col.as_str()));
            b.columns = columns;
            changed
        }
        BoardOp::Place { id, card, column } => {
            cards(std::slice::from_ref(&card))?;
            let b = board_mut(file, &id)?;
            let before = b.placed.get(&card).cloned();
            match &column {
                Some(col) => {
                    b.placed.insert(card, col.clone());
                }
                None => {
                    b.placed.remove(&card);
                }
            }
            before != column
        }
        BoardOp::Order { id, order } => {
            let mut order = cards(&order)?;
            let mut seen = std::collections::HashSet::new();
            order.retain(|c| seen.insert(c.clone()));
            let b = board_mut(file, &id)?;
            let changed = b.order != order;
            b.order = order;
            changed
        }
    };
    let b = file.boards.iter().find(|b| b.id == id).expect("changed above");
    Ok((changed, Applied { board: Some(b.clone().listed()), id, project: b.project.clone() }))
}

// ---- `codealong board show|move|arrange` ----
//
// The window works a board out (what it holds, its columns, its canvas's layout), so the command's
// requests go to the main window as an event, and its answer comes back through `board_answer`
// (lib.rs) to the connection that's waiting for it (src/boards/useBoardAsks.ts).

/// How a board is shown.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    #[default]
    Kanban,
    Canvas,
}

/// What `codealong board …` asks the main window.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Ask {
    /// What a board holds: its Kanban columns and their cards, or its canvas's frames and what's in each.
    Show { layout: Layout },
    /// Lays the board's canvas out in these frames, in order (canvas/model.ts `arrange`).
    Arrange { frames: Vec<Frame>, rest: Rest },
    /// Moves a card to a Kanban column, doing what dragging it there does (kanban.ts `kanbanDrop`),
    /// or with `dry_run` only says what that would be.
    Move {
        card: String,
        to: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<String>,
        #[serde(default)]
        dry_run: bool,
    },
}

/// A frame to make, with the cards that go in it, in order.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Frame {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub cards: Vec<Card>,
}

/// A card by what it shows and its key on the board.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Card {
    #[serde(rename = "ref")]
    pub kind: Kind,
    pub key: String,
}

/// What becomes of the cards on the canvas no frame names.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Rest {
    /// Laid out below the frames, as a tidy lays them out.
    #[default]
    Keep,
    /// Taken off the canvas, as a tidy takes off what's older.
    Off,
}

/// How long a request waits for the main window's answer.
const WAIT: Duration = Duration::from_secs(30);

/// The requests the main window hasn't answered yet, by id.
#[derive(Default)]
struct Asks {
    last: u64,
    waiting: HashMap<u64, Sender<Result<Value, String>>>,
}

impl Asks {
    fn open(&mut self) -> (u64, Receiver<Result<Value, String>>) {
        self.last += 1;
        let (tx, rx) = channel();
        self.waiting.insert(self.last, tx);
        (self.last, rx)
    }

    /// Hands an answer to the request it's for; false when nothing waits for it any more.
    fn answer(&mut self, id: u64, answer: Result<Value, String>) -> bool {
        self.waiting.remove(&id).is_some_and(|tx| tx.send(answer).is_ok())
    }
}

fn asks() -> &'static Mutex<Asks> {
    static ASKS: OnceLock<Mutex<Asks>> = OnceLock::new();
    ASKS.get_or_init(Default::default)
}

/// Asks the main window about the board `board` (an id) in `project`, and waits for its answer.
pub fn ask(app: &AppHandle, board: String, project: String, ask: Ask) -> Result<Value, String> {
    if app.get_webview_window("main").is_none() {
        return Err("Codealong's main window is gone, and it works boards out.".into());
    }
    let (id, rx) = asks().lock().map_err(|_| "Codealong's board requests are stuck.".to_string())?.open();
    let sent = app.emit_to("main", "board-ask", serde_json::json!({ "id": id, "board": board, "project": project, "ask": ask }));
    let got = sent.ok().and_then(|_| rx.recv_timeout(WAIT).ok());
    if let Ok(mut asks) = asks().lock() {
        asks.waiting.remove(&id);
    }
    got.unwrap_or_else(|| Err("Codealong's main window didn't answer in time. Ask again once it's open.".into()))
}

/// The main window's answer to a request (lib.rs `board_answer`).
pub fn answer(id: u64, answer: Result<Value, String>) {
    if let Ok(mut asks) = asks().lock() {
        asks.answer(id, answer);
    }
}

/// Tells every window a board changed, so each reads its project's boards again
/// (src/boards/useBoards.ts). `from` is the window that changed it, which has it already.
pub fn changed(app: &AppHandle, applied: &Applied, from: Option<&str>) {
    let _ = app.emit("boards-changed", serde_json::json!({ "project": applied.project, "id": applied.id, "from": from }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn file_with(boards: Vec<BoardDoc>) -> BoardsFile {
        BoardsFile { version: 1, boards, extra: Map::new() }
    }

    fn named(id: &str, name: &str, project: &str) -> BoardDoc {
        BoardDoc { id: id.into(), name: name.into(), project: project.into(), rule: None, created_at: 5, ..BoardDoc::of_project(project) }
    }

    fn run(file: &mut BoardsFile, op: BoardOp) -> Result<(bool, Applied)> {
        apply_to(file, op, 100, |_| "b-new".to_string())
    }

    #[test]
    fn a_card_is_named_by_its_kind_and_key() {
        assert_eq!(card_id(Kind::Pr, "acme/app#7"), "pr:acme/app#7");
        assert_eq!(parse_card("session:claude:claude2:abc"), Some((Kind::Session, "claude:claude2:abc")));
        assert_eq!(parse_card("worktree:/Users/me/projects/northwind-app"), Some((Kind::Worktree, "/Users/me/projects/northwind-app")));
        assert_eq!(parse_card("frame:x"), None);
        assert_eq!(parse_card("todo:"), None);
        assert_eq!(parse_card("todo"), None);
    }

    #[test]
    fn a_project_has_its_own_board_first_whether_or_not_the_file_has_it() {
        let file = file_with(vec![named("b-2", "Later", "Atlas"), named("b-1", "Release", "Northwind")]);
        let boards = listed(&file, "Northwind");
        assert_eq!(boards.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(), ["p:Northwind", "b-1"]);
        assert_eq!(boards[0].rule, Some(Rule::default()));
        assert!(boards[0].is_projects());
    }

    #[test]
    fn a_projects_board_is_written_as_it_first_changes_and_cant_be_deleted() {
        let mut file = BoardsFile::default();
        let (changed, out) = run(&mut file, BoardOp::Add { id: "p:Northwind".into(), cards: vec!["todo:t-1".into()] }).unwrap();
        assert!(changed);
        assert_eq!(out.project, "Northwind");
        assert_eq!(file.boards[0].added, ["todo:t-1"]);
        assert!(run(&mut file, BoardOp::Delete { id: "p:Northwind".into() }).is_err());
        assert!(run(&mut file, BoardOp::Delete { id: "p:Atlas".into() }).is_err());
        assert!(run(&mut file, BoardOp::Add { id: "b-gone".into(), cards: vec![] }).is_err());
    }

    #[test]
    fn adding_puts_back_what_was_taken_off_and_taking_off_keeps_it_off() {
        let mut file = file_with(vec![named("b-1", "Release", "Northwind")]);
        let add = |c: &str| BoardOp::Add { id: "b-1".into(), cards: vec![c.into()] };
        let remove = |c: &str| BoardOp::Remove { id: "b-1".into(), cards: vec![c.into()] };
        run(&mut file, add("session:s1")).unwrap();
        assert_eq!(file.boards[0].added_at.get("session:s1"), Some(&100));
        run(&mut file, BoardOp::Place { id: "b-1".into(), card: "session:s1".into(), column: Some("c1".into()) }).unwrap();
        run(&mut file, BoardOp::Order { id: "b-1".into(), order: vec!["session:s1".into(), "todo:t-1".into(), "session:s1".into()] }).unwrap();
        assert_eq!(file.boards[0].order, ["session:s1", "todo:t-1"]);
        run(&mut file, remove("session:s1")).unwrap();
        let b = &file.boards[0];
        assert!(b.added.is_empty() && b.placed.is_empty() && b.added_at.is_empty());
        assert_eq!((b.removed.clone(), b.order.clone()), (vec!["session:s1".to_string()], vec!["todo:t-1".to_string()]));
        let (changed, _) = run(&mut file, remove("session:s1")).unwrap();
        assert!(!changed);
        run(&mut file, add("session:s1")).unwrap();
        assert_eq!((file.boards[0].added.clone(), file.boards[0].removed.len()), (vec!["session:s1".to_string()], 0));
        assert!(run(&mut file, add("frame:f1")).is_err());
    }

    #[test]
    fn a_new_board_gets_an_id_of_its_own_and_a_name_no_other_board_there_has() {
        let mut file = file_with(vec![named("b-1", "Release", "Northwind")]);
        let new = |id: &str, name: &str, project: &str| BoardOp::Create { board: BoardDoc { id: id.into(), ..named("", name, project) } };
        let (_, out) = run(&mut file, new("b-1", "  Bugs   to fix ", "Northwind")).unwrap();
        assert_eq!(out.id, "b-new");
        assert_eq!(out.board.unwrap().name, "Bugs to fix");
        assert_eq!(file.boards[1].created_at, 100);
        assert!(run(&mut file, new("", "release", "Northwind")).is_err());
        // The project's own board has its name, written yet or not.
        assert!(run(&mut file, new("", "northwind", "Northwind")).is_err());
        assert!(run(&mut file, new("p:Atlas", "Release", "Atlas")).is_ok());
        assert_eq!(file.boards[2].id, "b-new");
        assert!(run(&mut file, new("", " ", "Atlas")).is_err());
        assert!(run(&mut file, BoardOp::Rename { id: "b-1".into(), name: "Bugs to fix".into() }).is_err());
        assert!(run(&mut file, BoardOp::Rename { id: "b-1".into(), name: "release".into() }).unwrap().0);
    }

    #[test]
    fn columns_need_ids_of_their_own_and_what_was_put_in_one_thats_gone_goes_back_to_the_rules() {
        let mut file = file_with(vec![named("b-1", "Release", "Northwind")]);
        let col = |id: &str| Column { id: id.into(), name: id.to_uppercase(), rule: None, sort: Sort::Hand, folded: false };
        run(&mut file, BoardOp::Columns { id: "b-1".into(), columns: Some(vec![col("next"), col("later")]) }).unwrap();
        run(&mut file, BoardOp::Place { id: "b-1".into(), card: "todo:t-1".into(), column: Some("later".into()) }).unwrap();
        run(&mut file, BoardOp::Place { id: "b-1".into(), card: "todo:t-2".into(), column: Some("next".into()) }).unwrap();
        run(&mut file, BoardOp::Columns { id: "b-1".into(), columns: Some(vec![col("next")]) }).unwrap();
        assert_eq!(file.boards[0].placed.keys().collect::<Vec<_>>(), ["todo:t-2"]);
        assert!(run(&mut file, BoardOp::Columns { id: "b-1".into(), columns: Some(vec![col("a"), col("a")]) }).is_err());
        let bad = Column { rule: Some(Rule { active: Some("2w".into()), ..Default::default() }), ..col("b") };
        assert!(run(&mut file, BoardOp::Columns { id: "b-1".into(), columns: Some(vec![bad]) }).is_err());
        run(&mut file, BoardOp::Place { id: "b-1".into(), card: "todo:t-2".into(), column: None }).unwrap();
        assert!(file.boards[0].placed.is_empty());
    }

    #[test]
    fn the_canvases_kept_before_boards_become_each_projects_boards_canvas() {
        let text = json!({ "version": 1, "canvases": {
            "Northwind": { "items": [{ "kind": "ref", "ref": "todo", "key": "t-1", "x": 0, "y": 0 }], "hidden": ["session:s9", "nonsense"], "camera": null },
            "Atlas": { "items": [] }
        } })
        .to_string();
        let file = from_canvases(&text);
        let north = file.boards.iter().find(|b| b.id == "p:Northwind").unwrap();
        assert_eq!(north.removed, ["session:s9"]);
        assert_eq!(north.canvas, json!({ "items": [{ "kind": "ref", "ref": "todo", "key": "t-1", "x": 0, "y": 0 }], "camera": null }));
        assert_eq!(north.rule, Some(Rule::default()));
        assert!(file.boards.iter().any(|b| b.id == "p:Atlas"));
        assert_eq!(from_canvases("{ nope").boards.len(), 0);
        assert_eq!(from_canvases("").boards.len(), 0);
    }

    #[test]
    fn a_board_reads_as_the_window_writes_it_and_keeps_what_it_doesnt_know() {
        // src/boards/model.ts `BoardDoc`.
        let raw = json!({
            "id": "b-1", "name": "Release", "project": "Northwind",
            "rule": { "labels": ["l-1"], "stages": ["review"], "active": "7d" },
            "added": ["todo:t-1"], "removed": [],
            "columns": [{ "id": "next", "name": "Next", "sort": "hand" }, { "id": "done", "name": "Done", "rule": { "stages": ["done"] }, "sort": "latest", "folded": true }],
            "placed": { "todo:t-1": "next" }, "order": ["todo:t-1"],
            "createdAt": 5, "fromTheFuture": 1
        });
        let b: BoardDoc = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(b.rule.as_ref().unwrap().stages, [Stage::Review]);
        assert_eq!(b.columns.as_ref().unwrap()[1].sort, Sort::Latest);
        assert_eq!(serde_json::to_value(&b).unwrap(), raw);
        // A board with no rule holds only what's added by hand.
        let hand: BoardDoc = serde_json::from_value(json!({ "id": "b-2", "name": "Hand", "project": "Northwind", "rule": null })).unwrap();
        assert_eq!(hand.rule, None);
    }

    #[test]
    fn a_change_reads_as_the_window_sends_it() {
        // src/boards/model.ts `BoardOp`.
        let op: BoardOp = serde_json::from_value(json!({ "op": "place", "id": "b-1", "card": "todo:t-1", "column": "next" })).unwrap();
        assert_eq!(op, BoardOp::Place { id: "b-1".into(), card: "todo:t-1".into(), column: Some("next".into()) });
        let op: BoardOp = serde_json::from_value(json!({ "op": "add", "id": "b-1", "cards": ["pr:acme/app#7"] })).unwrap();
        assert_eq!(op.board(), "b-1");
    }

    #[test]
    fn an_answer_goes_to_the_request_it_is_for_once() {
        let mut asks = Asks::default();
        let (one, first) = asks.open();
        let (two, second) = asks.open();
        assert!(asks.answer(two, Ok(json!(2))));
        assert_eq!(second.try_recv().unwrap(), Ok(json!(2)));
        assert!(first.try_recv().is_err());
        assert!(!asks.answer(two, Ok(json!(3))));
        assert!(asks.answer(one, Err("no".into())));
        assert_eq!(first.try_recv().unwrap(), Err("no".to_string()));
    }

    #[test]
    fn an_answer_nothing_waits_for_goes_nowhere() {
        let mut asks = Asks::default();
        let (id, rx) = asks.open();
        drop(rx);
        assert!(!asks.answer(id, Ok(Value::Null)));
        assert!(asks.waiting.is_empty());
    }

    #[test]
    fn a_request_reads_as_the_window_takes_it() {
        // src/boards/useBoardAsks.ts `Ask` and canvas/model.ts `ArrangeFrame`.
        assert_eq!(serde_json::to_value(Ask::Show { layout: Layout::Canvas }).unwrap(), json!({ "kind": "show", "layout": "canvas" }));
        let arrange = Ask::Arrange { frames: vec![Frame { title: "Release".into(), color: None, note: Some("Two PRs".into()), cards: vec![Card { kind: Kind::Pr, key: "acme/app#7".into() }] }], rest: Rest::Off };
        assert_eq!(
            serde_json::to_value(&arrange).unwrap(),
            json!({ "kind": "arrange", "frames": [{ "title": "Release", "note": "Two PRs", "cards": [{ "ref": "pr", "key": "acme/app#7" }] }], "rest": "off" })
        );
        let moved = Ask::Move { card: "todo:t-1".into(), to: "done".into(), before: None, dry_run: true };
        assert_eq!(serde_json::to_value(&moved).unwrap(), json!({ "kind": "move", "card": "todo:t-1", "to": "done", "dryRun": true }));
    }
}
