//! Which review comments on a PR ask for something: a change, or an answer before it can merge
//! (`[jev] pull_requests`). The PR panel shows the comments first and asks here after, so it
//! never waits on TypeSafe. Every comment is one yes/no question, all in one request, and each
//! answer is kept by the comment's id and text in `jev-prs.json`, so a PR opened again asks only
//! about what's new or edited.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use anyhow::Result;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::jev;

/// Comments asked about at most, newest first.
const MAX: usize = 60;
/// How much of a comment goes in.
const BODY_CHARS: usize = 1200;
/// Answers kept at most; the oldest go first.
const KEEP: usize = 3000;

pub const QUESTION: &str = "Does `comment` ask the pull request's author to change something, or to answer a question before it can merge?";

/// A comment as the PR panel has it.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CommentIn {
    pub id: String,
    pub author: String,
    pub body: String,
    #[serde(default)]
    pub path: Option<String>,
}

/// A comment's place in the cache: its id and a hash of its text, so an edit asks again.
fn cache_key(c: &CommentIn) -> String {
    let hash = Sha256::digest(c.body.as_bytes());
    format!("{}:{}", c.id, hash.iter().take(8).map(|b| format!("{b:02x}")).collect::<String>())
}

fn cache_path() -> PathBuf {
    crate::config::support_dir().join("jev-prs.json")
}

/// Answers by cache key, in the order they were added.
#[derive(Default, serde::Serialize, Deserialize)]
struct Cache {
    answers: BTreeMap<String, (f64, i64)>,
}

fn load() -> Cache {
    std::fs::read_to_string(cache_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(cache: &mut Cache) {
    if cache.answers.len() > KEEP {
        let mut by_age: Vec<(String, i64)> = cache.answers.iter().map(|(k, (_, at))| (k.clone(), *at)).collect();
        by_age.sort_by_key(|(_, at)| *at);
        for (k, _) in by_age.into_iter().take(cache.answers.len() - KEEP) {
            cache.answers.remove(&k);
        }
    }
    if let Ok(text) = serde_json::to_string(cache) {
        let _ = std::fs::create_dir_all(crate::config::support_dir());
        let _ = std::fs::write(cache_path(), text);
    }
}

/// The request for comments not answered yet: one noul each, the comment in its instructions.
pub fn request(model: &str, pr_title: &str, comments: &[&CommentIn]) -> Value {
    let mut questions = Map::new();
    for (i, c) in comments.iter().enumerate() {
        let mut comment = Map::new();
        comment.insert("by".into(), json!(c.author));
        if let Some(p) = &c.path {
            comment.insert("on file".into(), json!(p));
        }
        comment.insert("text".into(), json!(jev::clip(&c.body, BODY_CHARS)));
        questions.insert(
            format!("c{}", i + 1),
            json!({
                "type": "noul",
                "instructions": { "comment": Value::Object(comment), "question": QUESTION },
                "criteria": {
                    "true": "It asks for a change, a fix, a test, or an answer the author owes before merging.",
                    "false": "Praise, an approval, a note for later, a question already answered, or a reply by the author.",
                },
            }),
        );
    }
    jev::body(model, json!({ "pull request": pr_title }), Value::Object(questions))
}

/// How likely each comment is to ask for something, by comment id. Your own comments and
/// comments on resolved threads aren't passed in; the panel leaves them out.
pub fn asks(model: &str, pr_title: &str, comments: &[CommentIn]) -> Result<HashMap<String, f64>> {
    let mut cache = load();
    let mut out = HashMap::new();
    let mut todo: Vec<&CommentIn> = Vec::new();
    for c in comments.iter().take(MAX) {
        match cache.answers.get(&cache_key(c)) {
            Some((p, _)) => {
                out.insert(c.id.clone(), *p);
            }
            None => todo.push(c),
        }
    }
    if todo.is_empty() {
        return Ok(out);
    }
    let answer = jev::ask(&request(model, pr_title, &todo))?;
    let now = crate::util::now_ms();
    for (i, c) in todo.iter().enumerate() {
        if let Some(p) = jev::noul(&answer, &format!("c{}", i + 1)) {
            out.insert(c.id.clone(), p);
            cache.answers.insert(cache_key(c), (p, now));
        }
    }
    save(&mut cache);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(id: &str, body: &str, path: Option<&str>) -> CommentIn {
        CommentIn { id: id.into(), author: "ana".into(), body: body.into(), path: path.map(String::from) }
    }

    #[test]
    fn asks_one_noul_per_comment_with_the_comment_in_the_question() {
        let a = c("C1", "Please add a test", Some("src/a.ts"));
        let b = c("C2", "Looks great", None);
        let req = request("jev-latest", "Fix login", &[&a, &b]);
        assert_eq!(req["state"]["pull request"], "Fix login");
        assert_eq!(req["questions"]["c1"]["type"], "noul");
        assert_eq!(req["questions"]["c1"]["instructions"]["comment"]["on file"], "src/a.ts");
        assert_eq!(req["questions"]["c2"]["instructions"]["comment"]["text"], "Looks great");
        assert_eq!(req["questions"]["c2"]["instructions"]["question"], QUESTION);
        assert!(req["questions"]["c2"]["instructions"]["comment"].get("on file").is_none());
    }

    #[test]
    fn an_edited_comment_is_asked_about_again() {
        assert_eq!(cache_key(&c("C1", "a", None)), cache_key(&c("C1", "a", None)));
        assert_ne!(cache_key(&c("C1", "a", None)), cache_key(&c("C1", "b", None)));
    }
}
