//! The Canvas view's documents: where you put each session, worktree, todo and PR, and your
//! notes and frames, one document per project. Kept in `canvas.json` next to the config. The
//! window owns the documents' shape (src/canvas/model.ts); the engine only keeps them, so a
//! newer window's fields survive an older engine.

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{bail, Result};
use serde_json::{Map, Value};

use crate::config::support_dir;

/// Windows save as you move things; this keeps two of them from interleaving a read and a write.
static WRITING: Mutex<()> = Mutex::new(());

pub fn path() -> PathBuf {
    support_dir().join("canvas.json")
}

/// A project's document, or null when it has none yet.
pub fn load(project: &str) -> Result<Value> {
    Ok(document(&read()?, project))
}

/// Replaces a project's document, leaving the others as they are.
pub fn save(project: &str, doc: Value) -> Result<()> {
    let _held = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let text = with_document(&read()?, project, doc)?;
    std::fs::create_dir_all(support_dir())?;
    let p = path();
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &p)?;
    Ok(())
}

fn read() -> Result<String> {
    match std::fs::read_to_string(path()) {
        Ok(t) => Ok(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => bail!("Couldn't read canvas.json: {e}"),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_project_is_saved_beside_the_others() {
        let first = with_document("", "Codealong", json!({ "items": [1] })).unwrap();
        let both = with_document(&first, "Kestrel", json!({ "items": [2] })).unwrap();
        assert_eq!(document(&both, "Codealong"), json!({ "items": [1] }));
        assert_eq!(document(&both, "Kestrel"), json!({ "items": [2] }));
        let replaced = with_document(&both, "Codealong", json!({ "items": [] })).unwrap();
        assert_eq!(document(&replaced, "Codealong"), json!({ "items": [] }));
        assert_eq!(document(&replaced, "Kestrel"), json!({ "items": [2] }));
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
