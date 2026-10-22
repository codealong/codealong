//! Boards: named sets of work in a project, each shown as Kanban or as Canvas (src/boards/). A
//! board holds what its rule takes in and what you added by hand, less what you took off; its
//! Kanban columns, where you put cards by hand and in what order; and its canvas, where each card
//! sits among your notes and frames. A project has only the boards made in it: none until you
//! make one (`without_own_boards` says what became of the board each project had of its own).
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

/// The file's version: 2 since projects stopped having boards of their own (`without_own_boards`).
const VERSION: u32 = 2;

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
    /// When each card was added by hand.
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
    /// A new board in `project`, holding only what's added by hand, with no id yet: `apply` gives
    /// it one as it makes it.
    pub fn new(name: &str, project: &str) -> BoardDoc {
        BoardDoc {
            id: String::new(),
            name: name.to_string(),
            project: project.to_string(),
            rule: None,
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

/// A change to a board. Each but `Create` names its board by id.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum BoardOp {
    /// A new board. An id that's empty or taken is replaced.
    Create { board: BoardDoc },
    Rename { id: String, name: String },
    /// Moves a board to another project, or to All projects: it shows that project's work from
    /// then on.
    Project { id: String, project: String },
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
            BoardOp::Rename { id, .. } | BoardOp::Project { id, .. } | BoardOp::Delete { id } | BoardOp::Add { id, .. } | BoardOp::Remove { id, .. } | BoardOp::Rule { id, .. } | BoardOp::Columns { id, .. } | BoardOp::Place { id, .. } | BoardOp::Order { id, .. } => id,
        }
    }
}

/// What a change did: the board as it is now, or none once it's deleted, and its project, and
/// the project it left when it moved to this one.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub board: Option<BoardDoc>,
    pub id: String,
    pub project: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<String>,
}

/// The file as it is, or for none yet, the canvases kept before there were boards.
fn read() -> Result<BoardsFile> {
    if let Some(file) = store::read::<BoardsFile>(&path(), "boards")? {
        return Ok(without_own_boards(file));
    }
    // A canvas file that doesn't read gives no boards, and is left as it is.
    Ok(without_own_boards(from_canvases(&store::text(&canvases_path()).ok().flatten().unwrap_or_default())))
}

/// Until 2026-10-22 every project had a board of its own, `p:<project>`, that held everything in
/// it and couldn't be deleted, and a file of version 1 kept one once it first changed. One of
/// those with something of yours on it (cards added or taken off, its own columns, cards put in a
/// column or an order, another name or rule, notes on its canvas or frames a tidy didn't draw)
/// stays, as a board like any other, its id and all; the rest, which held nothing but what the
/// project had, go. Version 2 is written with the next change, and from then on an id like that
/// is only an id.
fn without_own_boards(mut file: BoardsFile) -> BoardsFile {
    if file.version < VERSION {
        file.boards.retain(|b| b.id != format!("p:{}", b.project) || has_yours(b));
    }
    file
}

fn has_yours(b: &BoardDoc) -> bool {
    // A canvas brings on what its board holds by itself, and a tidy draws frames round it
    // (`auto`, canvas/model.ts `FrameItem`); a card moved or kept there (`kept`, `RefItem`), a
    // note or another frame was put there.
    let yours = |it: &Value| {
        let is = |field: &str| it.get(field) == Some(&Value::Bool(true));
        if it.get("kind").and_then(Value::as_str) == Some("ref") {
            is("kept")
        } else {
            !is("auto")
        }
    };
    let drawn = b.canvas.get("items").and_then(Value::as_array).is_some_and(|items| items.iter().any(yours));
    b.name != b.project || b.rule != Some(Rule::default()) || !b.added.is_empty() || !b.removed.is_empty() || b.columns.is_some() || !b.placed.is_empty() || !b.order.is_empty() || drawn
}

/// Each project's canvas from canvas.json, as the canvas of a board of the project's own, which
/// `without_own_boards` keeps only when its canvas has notes or frames of yours or had cards
/// taken off.
/// What was taken off a canvas is taken off its board.
fn from_canvases(text: &str) -> BoardsFile {
    let mut file = BoardsFile { version: 1, ..Default::default() };
    let Ok(Value::Object(root)) = serde_json::from_str::<Value>(text) else { return file };
    let Some(Value::Object(canvases)) = root.get("canvases") else { return file };
    for (project, doc) in canvases {
        let mut doc = doc.clone();
        let mut board = BoardDoc { id: format!("p:{project}"), rule: Some(Rule::default()), ..BoardDoc::new(project, project) };
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
        file.version = VERSION;
        store::save(&path(), &file)?;
    }
    Ok(out)
}

/// A project's boards as they were made, without their canvases. All projects lists every
/// project's after its own, as it lists every project's sessions: a board shows its own project's
/// work wherever it's shown (src/boardLists.ts `useBoardLists`).
pub fn list(project: &str) -> Result<Vec<BoardDoc>> {
    Ok(listed(&read()?, project))
}

fn listed(file: &BoardsFile, project: &str) -> Vec<BoardDoc> {
    let every = project == crate::route::ALL_PROJECTS;
    let mut out: Vec<&BoardDoc> = file.boards.iter().filter(|b| every || b.project == project).collect();
    // Under All projects, its own boards first, then each project's in turn.
    out.sort_by(|a, b| (a.project != project).cmp(&(b.project != project)).then_with(|| a.project.cmp(&b.project)).then(a.created_at.cmp(&b.created_at)));
    out.into_iter().map(|b| b.clone().listed()).collect()
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

fn board_mut<'a>(file: &'a mut BoardsFile, id: &str) -> Result<&'a mut BoardDoc> {
    match file.boards.iter_mut().find(|b| b.id == id) {
        Some(b) => Ok(b),
        None => bail!("There's no board {id}."),
    }
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
    if file.boards.iter().any(taken) {
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
    let mut left = None;
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
            if board.id.is_empty() || file.boards.iter().any(|b| b.id == board.id) {
                board.id = new_id(now);
            }
            board.added = cards(&board.added)?;
            board.removed = cards(&board.removed)?;
            for c in &board.added {
                board.added_at.entry(c.clone()).or_insert(now);
            }
            board.created_at = now;
            let out = Applied { board: Some(board.clone().listed()), id: board.id.clone(), project: board.project.clone(), left: None };
            file.boards.push(board);
            return Ok((true, out));
        }
        BoardOp::Delete { id } => {
            let Some(at) = file.boards.iter().position(|b| b.id == id) else { bail!("There's no board {id}.") };
            let gone = file.boards.remove(at);
            return Ok((true, Applied { board: None, id, project: gone.project, left: None }));
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
        BoardOp::Project { id, project } => {
            let project = project.trim().to_string();
            if project.is_empty() {
                bail!("A board needs a project.");
            }
            let b = board_mut(file, &id)?;
            let (name, from) = (b.name.clone(), b.project.clone());
            if from == project {
                false
            } else {
                name_free(file, &project, &name, &id)?;
                board_mut(file, &id)?.project = project;
                left = Some(from);
                true
            }
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
    Ok((changed, Applied { board: Some(b.clone().listed()), id, project: b.project.clone(), left }))
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
    /// Taken off the canvas and the board, which would show them there again.
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
    // A board that moved is told to the windows of the project it left too, which list it no more.
    for project in std::iter::once(&applied.project).chain(applied.left.as_ref()) {
        let _ = app.emit("boards-changed", serde_json::json!({ "project": project, "id": applied.id, "from": from }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn file_with(boards: Vec<BoardDoc>) -> BoardsFile {
        BoardsFile { version: VERSION, boards, extra: Map::new() }
    }

    fn named(id: &str, name: &str, project: &str) -> BoardDoc {
        BoardDoc { id: id.into(), created_at: 5, ..BoardDoc::new(name, project) }
    }

    /// A project's own board as a file of version 1 kept it.
    fn own(project: &str) -> BoardDoc {
        BoardDoc { id: format!("p:{project}"), rule: Some(Rule::default()), ..BoardDoc::new(project, project) }
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
    fn a_project_has_only_the_boards_made_in_it() {
        let file = file_with(vec![named("b-2", "Later", "Atlas"), named("b-1", "Release", "Northwind")]);
        let ids = |p: &str| listed(&file, p).into_iter().map(|b| b.id).collect::<Vec<_>>();
        assert_eq!(ids("Northwind"), ["b-1"]);
        assert!(ids("Kestrel").is_empty());
        assert!(listed(&BoardsFile::default(), crate::route::ALL_PROJECTS).is_empty());
    }

    #[test]
    fn all_projects_lists_every_projects_boards_after_its_own() {
        let all = crate::route::ALL_PROJECTS;
        let file = file_with(vec![named("b-3", "Later", "Northwind"), named("b-2", "Ideas", all), BoardDoc { created_at: 1, ..named("b-1", "Release", "Northwind") }, named("b-4", "Site", "Atlas")]);
        let ids = |p: &str| listed(&file, p).into_iter().map(|b| b.id).collect::<Vec<_>>();
        assert_eq!(ids(all), ["b-2", "b-4", "b-1", "b-3"]);
        assert_eq!(ids("Northwind"), ["b-1", "b-3"]);
    }

    #[test]
    fn a_change_to_a_board_that_isnt_there_is_turned_down() {
        let mut file = file_with(vec![named("b-1", "Release", "Northwind")]);
        // A project's own board isn't made by a change to it any more.
        assert!(run(&mut file, BoardOp::Add { id: "p:Northwind".into(), cards: vec!["todo:t-1".into()] }).unwrap_err().to_string().contains("There's no board p:Northwind"));
        assert!(run(&mut file, BoardOp::Delete { id: "p:Atlas".into() }).is_err());
        assert!(run(&mut file, BoardOp::Add { id: "b-gone".into(), cards: vec![] }).is_err());
        assert_eq!(file.boards.len(), 1);
        let (changed, out) = run(&mut file, BoardOp::Delete { id: "b-1".into() }).unwrap();
        assert!(changed && out.board.is_none() && file.boards.is_empty());
    }

    #[test]
    fn a_projects_own_board_from_before_stays_only_with_something_of_yours_on_it() {
        let canvas = |kinds: &[&str]| json!({ "items": kinds.iter().map(|k| json!({ "kind": k })).collect::<Vec<_>>() });
        // What a tidy drew round the cards is the canvas's, not yours; a card you moved is yours.
        let tidied = json!({ "items": [{ "kind": "ref" }, { "kind": "frame", "auto": true }] });
        let moved = json!({ "items": [{ "kind": "ref", "kept": true }, { "kind": "frame", "auto": true }] });
        let yours = vec![
            BoardDoc { canvas: canvas(&["ref", "frame"]), ..own("Atlas") },
            BoardDoc { canvas: moved, ..own("Birch") },
            BoardDoc { name: "Kestrel, renamed".into(), ..own("Kestrel") },
            BoardDoc { removed: vec!["session:s9".into()], ..own("Harbor") },
            BoardDoc { columns: Some(vec![]), ..own("Juniper") },
            BoardDoc { rule: Some(Rule { stages: vec![Stage::Review], ..Default::default() }), ..own("Larch") },
        ];
        let mut boards = vec![BoardDoc { canvas: tidied, ..own("Northwind") }, own(crate::route::ALL_PROJECTS), named("b-1", "Release", "Northwind")];
        boards.extend(yours);
        let ids = |f: &BoardsFile| f.boards.iter().map(|b| b.id.as_str()).collect::<Vec<_>>().join(" ");
        let before = BoardsFile { version: 1, boards, extra: Map::new() };
        let after = without_own_boards(before.clone());
        assert_eq!(ids(&after), "b-1 p:Atlas p:Birch p:Kestrel p:Harbor p:Juniper p:Larch");
        // Once the file is of version 2, a board with an id like that is a board like any other.
        let mut file = BoardsFile { version: VERSION, ..before };
        assert_eq!(without_own_boards(file.clone()).boards.len(), 9);
        assert!(run(&mut file, BoardOp::Delete { id: "p:Northwind".into() }).unwrap().0);
        assert_eq!(run(&mut file, BoardOp::Project { id: "p:Atlas".into(), project: "Northwind".into() }).unwrap().1.left.as_deref(), Some("Atlas"));
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
        let with: BoardDoc = BoardDoc { added: vec!["session:s1".into()], ..named("", "With it", "Atlas") };
        run(&mut file, BoardOp::Create { board: with }).unwrap();
        // What a board is made with was added as it was made.
        assert_eq!(file.boards.last().unwrap().added_at.get("session:s1"), Some(&100));
        file.boards.pop();
        assert_eq!(out.board.unwrap().name, "Bugs to fix");
        assert_eq!(file.boards[1].created_at, 100);
        assert!(run(&mut file, new("", "release", "Northwind")).is_err());
        // A board may be named for its project.
        assert!(run(&mut file, new("", "northwind", "Northwind")).is_ok());
        assert!(run(&mut file, new("", " ", "Atlas")).is_err());
        assert!(run(&mut file, BoardOp::Rename { id: "b-1".into(), name: "Bugs to fix".into() }).is_err());
        assert!(run(&mut file, BoardOp::Rename { id: "b-1".into(), name: "release".into() }).unwrap().0);
    }

    #[test]
    fn a_board_moves_to_another_project_with_what_it_has_and_says_the_one_it_left() {
        let mut file = file_with(vec![BoardDoc { added: vec!["todo:t-1".into()], ..named("b-1", "Release", "Northwind") }, named("b-2", "Release", "Atlas"), named("b-3", "Later", "Atlas")]);
        let to = |id: &str, project: &str| BoardOp::Project { id: id.into(), project: project.into() };
        let (changed, out) = run(&mut file, to("b-3", "Northwind")).unwrap();
        assert!(changed);
        assert_eq!((out.project.as_str(), out.left.as_deref()), ("Northwind", Some("Atlas")));
        assert_eq!(file.boards[2].project, "Northwind");
        // Already there, it doesn't move.
        let (changed, out) = run(&mut file, to("b-3", "Northwind")).unwrap();
        assert!(!changed && out.left.is_none());
        // Where a board has its name it can't go, nor nowhere.
        assert!(run(&mut file, to("b-1", "Atlas")).unwrap_err().to_string().contains("Atlas already has a board called Release"));
        assert!(run(&mut file, to("b-1", " ")).is_err());
        let (_, out) = run(&mut file, to("b-1", crate::route::ALL_PROJECTS)).unwrap();
        assert_eq!(out.board.unwrap().added, ["todo:t-1"]);
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
    fn the_canvases_kept_before_boards_become_boards_where_they_have_something_of_yours() {
        let text = json!({ "version": 1, "canvases": {
            "Northwind": { "items": [{ "kind": "ref", "ref": "todo", "key": "t-1", "x": 0, "y": 0 }], "hidden": ["session:s9", "nonsense"], "camera": null },
            "Atlas": { "items": [{ "kind": "ref", "ref": "todo", "key": "t-2", "x": 0, "y": 0 }] },
            "Kestrel": { "items": [{ "kind": "note", "text": "Ship it" }] }
        } })
        .to_string();
        let file = without_own_boards(from_canvases(&text));
        let mut ids: Vec<&str> = file.boards.iter().map(|b| b.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["p:Kestrel", "p:Northwind"]);
        let north = file.boards.iter().find(|b| b.id == "p:Northwind").unwrap();
        assert_eq!((north.name.as_str(), north.project.as_str()), ("Northwind", "Northwind"));
        assert_eq!(north.removed, ["session:s9"]);
        assert_eq!(north.canvas, json!({ "items": [{ "kind": "ref", "ref": "todo", "key": "t-1", "x": 0, "y": 0 }], "camera": null }));
        assert_eq!(north.rule, Some(Rule::default()));
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
        let op: BoardOp = serde_json::from_value(json!({ "op": "project", "id": "b-1", "project": "Atlas" })).unwrap();
        assert_eq!(op, BoardOp::Project { id: "b-1".into(), project: "Atlas".into() });
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
