//! The Canvas view's documents: where you put each session, worktree, todo and PR, and your
//! notes and frames, one document per project. Kept in `canvas.json` next to the config. The
//! window owns the documents' shape (src/canvas/model.ts); the engine only keeps them, so a
//! newer window's fields survive an older engine. `codealong canvas …` asks the main window too.

use std::collections::HashMap;
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
    support_dir().join("canvas.json")
}

/// A project's document, or null when it has none yet.
pub fn load(project: &str) -> Result<Value> {
    Ok(document(&read()?, project))
}

/// Replaces a project's document, leaving the others as they are.
pub fn save(project: &str, doc: Value) -> Result<()> {
    // Windows save as you move things; the lock keeps two of them from interleaving a read and
    // a write.
    let _lock = store::lock(&path())?;
    let text = with_document(&read()?, project, doc)?;
    store::write(&path(), text)
}

fn read() -> Result<String> {
    Ok(store::text(&path())?.unwrap_or_default())
}

fn document(text: &str, project: &str) -> Value {
    serde_json::from_str::<Value>(text).ok().and_then(|v| v.get("canvases")?.get(project).cloned()).unwrap_or(Value::Null)
}

/// The file's text with one project's document replaced. A file that doesn't parse is an error,
/// so a hand edit gone wrong isn't overwritten with one project's canvas.
fn with_document(text: &str, project: &str, doc: Value) -> Result<String> {
    if !doc.is_object() {
        bail!("A canvas is a JSON object.");
    }
    let mut file = if text.trim().is_empty() {
        Value::Object(Map::new())
    } else {
        match serde_json::from_str::<Value>(text) {
            Ok(v) if v.is_object() => v,
            _ => bail!("canvas.json doesn't read as canvases. Codealong won't change it until it does."),
        }
    };
    let root = file.as_object_mut().expect("checked above");
    root.insert("version".into(), Value::from(1));
    let canvases = root.entry("canvases").or_insert_with(|| Value::Object(Map::new()));
    if !canvases.is_object() {
        *canvases = Value::Object(Map::new());
    }
    canvases.as_object_mut().expect("just made").insert(project.to_string(), doc);
    Ok(serde_json::to_string_pretty(&file)?)
}

// ---- `codealong canvas …` ----
//
// The window works a canvas out (its shape and its layout are model.ts's), so the command's
// requests go to the main window as an event, and its answer comes back through
// `canvas_answer` (lib.rs) to the connection that's waiting for it (src/canvas/useCanvasOps.ts).

/// What `codealong canvas …` asks the main window.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Op {
    /// What a project's canvas holds: its frames, and the cards and notes in each.
    Show,
    /// Lays the canvas out in these frames, in order (model.ts `arrange`).
    Arrange { frames: Vec<Frame>, rest: Rest },
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

/// A card by what it shows and its key on the board: a worktree's id (its folder), a session's
/// key, a todo's id, a PR's `owner/name#number`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Card {
    #[serde(rename = "ref")]
    pub kind: Kind,
    pub key: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Worktree,
    Session,
    Todo,
    Pr,
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

/// Asks the main window, and waits for its answer: about `project` by name, or the one the window
/// shows for none.
pub fn ask(app: &AppHandle, project: Option<String>, op: Op) -> Result<Value, String> {
    if app.get_webview_window("main").is_none() {
        return Err("Codealong's main window is gone, and it holds the canvas.".into());
    }
    let (id, rx) = asks().lock().map_err(|_| "Codealong's canvas requests are stuck.".to_string())?.open();
    let sent = app.emit_to("main", "canvas-ask", serde_json::json!({ "id": id, "project": project, "op": op }));
    let got = sent.ok().and_then(|_| rx.recv_timeout(WAIT).ok());
    if let Ok(mut asks) = asks().lock() {
        asks.waiting.remove(&id);
    }
    got.unwrap_or_else(|| Err("Codealong's main window didn't answer in time. Ask again once it's open.".into()))
}

/// The main window's answer to a request (lib.rs `canvas_answer`).
pub fn answer(id: u64, answer: Result<Value, String>) {
    if let Ok(mut asks) = asks().lock() {
        asks.answer(id, answer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        // useCanvasOps.ts `Op` and model.ts `ArrangeFrame`.
        assert_eq!(serde_json::to_value(Op::Show).unwrap(), json!({ "kind": "show" }));
        let arrange = Op::Arrange { frames: vec![Frame { title: "Release".into(), color: None, note: Some("Two PRs".into()), cards: vec![Card { kind: Kind::Pr, key: "acme/app#7".into() }] }], rest: Rest::Off };
        assert_eq!(
            serde_json::to_value(&arrange).unwrap(),
            json!({ "kind": "arrange", "frames": [{ "title": "Release", "note": "Two PRs", "cards": [{ "ref": "pr", "key": "acme/app#7" }] }], "rest": "off" })
        );
    }

    #[test]
    fn a_project_is_saved_beside_the_others() {
        let first = with_document("", "Codealong", json!({ "items": [1] })).unwrap();
        let both = with_document(&first, "Spur", json!({ "items": [2] })).unwrap();
        assert_eq!(document(&both, "Codealong"), json!({ "items": [1] }));
        assert_eq!(document(&both, "Spur"), json!({ "items": [2] }));
        let replaced = with_document(&both, "Codealong", json!({ "items": [] })).unwrap();
        assert_eq!(document(&replaced, "Codealong"), json!({ "items": [] }));
        assert_eq!(document(&replaced, "Spur"), json!({ "items": [2] }));
    }

    #[test]
    fn a_missing_canvas_is_null() {
        assert_eq!(document("", "Codealong"), Value::Null);
        assert_eq!(document("{\"canvases\":{}}", "Codealong"), Value::Null);
    }

    #[test]
    fn a_broken_file_is_left_alone() {
        assert!(with_document("{ nope", "Codealong", json!({})).is_err());
        assert!(with_document("", "Codealong", json!([1])).is_err());
    }
}
