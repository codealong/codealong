//! What the Codex app holds to send a thread once the turn it's taking ends: its follow-up queue,
//! `queue_1.sqlite` in the account's folder (26.1007). The app's box adds to it while a turn runs
//! when Codex is set to queue follow-ups, and so does a send from Codealong then
//! (`appserver::queue`). The app takes a message out as it sends it, as the next turn's prompt or
//! steered into the turn, and only then does the rollout have it. A thread's session lists what's
//! there (`Session::queued`), so its conversation shows it whoever queued it.
//!
//! `queued_items` has a row a message: `id`, `thread_id`, `queue_order`, `created_at_ms`, and
//! `payload_json`, `{"UserInput":{"content":[…],"client_id":"…"}}`, its parts as `turn/start` takes
//! them, an image the app-server was handed as a path kept as `{"type":"image","image_url":
//! "data:…"}`, and `client_id` the `clientUserMessageId` it was added with, which a message
//! Codealong queued is known by (0.160 on a scratch CODEX_HOME, 2026-10-17). See
//! .claude/rules/codex.md.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::Connection;
use serde_json::Value;

use crate::model::QueuedMessage;

/// The queue's database, in the account's folder.
pub const DB: &str = "queue_1.sqlite";

/// Each thread's queued messages, by its id, in the order they'll go. Nothing when there's no
/// queue or it can't be read.
pub fn read(codex_home: &Path) -> HashMap<String, Vec<QueuedMessage>> {
    crate::harness::open_database(&codex_home.join(DB)).map(|db| read_from(&db)).unwrap_or_default()
}

fn read_from(db: &Connection) -> HashMap<String, Vec<QueuedMessage>> {
    let rows = || -> rusqlite::Result<Vec<(String, String, String, i64)>> {
        let mut stmt = db.prepare("SELECT id, thread_id, payload_json, created_at_ms FROM queued_items ORDER BY thread_id, queue_order")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.flatten().collect();
        Ok(rows)
    };
    let mut out: HashMap<String, Vec<QueuedMessage>> = HashMap::new();
    for (row, thread, payload, at) in rows().unwrap_or_default() {
        let Ok(v) = serde_json::from_str::<Value>(&payload) else { continue };
        let Some(text) = words(&v) else { continue };
        let id = v.pointer("/UserInput/client_id").and_then(Value::as_str).filter(|id| !id.is_empty()).map_or(row, String::from);
        out.entry(thread).or_default().push(QueuedMessage { id, at, text });
    }
    out
}

/// A queued message's words, each image after them as `[Image #n]`, as a prompt with images reads;
/// None for a payload that isn't a message.
fn words(v: &Value) -> Option<String> {
    let parts = v.pointer("/UserInput/content")?.as_array()?;
    let kind = |p: &Value| p.get("type").and_then(Value::as_str).unwrap_or_default().to_string();
    let mut said: Vec<String> = parts.iter().filter(|p| kind(p) == "text").filter_map(|p| p.get("text")?.as_str()).map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
    let images = parts.iter().filter(|p| matches!(kind(p).as_str(), "image" | "localImage" | "local_image")).count();
    said.extend((1..=images).map(|n| format!("[Image #{n}]")));
    (!said.is_empty()).then(|| said.join("\n\n"))
}

/// A queue as Codex keeps it in `home`, its rows given as (thread, order, when queued, payload).
#[cfg(test)]
pub(super) fn queue_at(home: &Path, rows: &[(&str, i64, i64, &str)]) {
    let db = Connection::open(home.join(DB)).unwrap();
    db.execute_batch("CREATE TABLE IF NOT EXISTS queued_items (id TEXT PRIMARY KEY NOT NULL, thread_id TEXT NOT NULL, payload_json TEXT NOT NULL, queue_order INTEGER NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL);").unwrap();
    for (i, (thread, order, at, payload)) in rows.iter().enumerate() {
        db.execute("INSERT INTO queued_items VALUES (?1, ?2, ?3, ?4, ?5, ?5)", rusqlite::params![format!("q{i}"), thread, payload, order, at]).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_each_threads_queued_messages_in_the_order_they_go() {
        let home = crate::store::test_dir("codex-queue");
        // As Codex wrote them: a message with an image the app-server read in, and one moved ahead.
        let image = r#"{"UserInput":{"content":[{"type":"text","text":"and the docs\nplease","text_elements":[]},{"type":"image","image_url":"data:image/png;base64,iVBORw0KGgo="}],"client_id":"u1"}}"#;
        queue_at(
            &home,
            &[
                ("t1", 1, 1_000, image),
                ("t1", 0, 2_000, r#"{"UserInput":{"content":[{"type":"text","text":"second\n","text_elements":[]}],"client_id":"u2"}}"#),
                // Without a client id it's known by its row's.
                ("t2", 0, 3_000, r#"{"UserInput":{"content":[{"type":"image","image_url":"data:image/png;base64,iVBORw0KGgo="}]}}"#),
                // Something else the queue may hold one day isn't a message to show.
                ("t3", 0, 4_000, r#"{"Compact":{}}"#),
            ],
        );
        let queued = read(&home);
        let said = |t: &str| queued.get(t).map(|q| q.iter().map(|m| (m.id.as_str(), m.at, m.text.as_str())).collect::<Vec<_>>());
        assert_eq!(said("t1"), Some(vec![("u2", 2_000, "second"), ("u1", 1_000, "and the docs\nplease\n\n[Image #1]")]));
        assert_eq!(said("t2"), Some(vec![("q2", 3_000, "[Image #1]")]));
        assert_eq!(said("t3"), None);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn no_queue_is_nothing_queued() {
        let home = crate::store::test_dir("codex-no-queue");
        assert!(read(&home).is_empty());
        // A database of another shape, as a later Codex might make it.
        Connection::open(home.join(DB)).unwrap().execute_batch("CREATE TABLE other (id TEXT);").unwrap();
        assert!(read(&home).is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }
}
