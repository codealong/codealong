//! OpenAI's Decisions API (developers.openai.com/api/docs/guides/decisions, read 2026-11-19):
//! `POST /v1/decisions` with `model`, `input` and `questions`, on `gpt-6-luna`, its only model.
//!
//! - Jev's state is `input`: one user message, its content the state as `input_json` (an object
//!   or a list of objects, at most 256 KiB written out), else as `input_text`. With images, the
//!   state goes as text cut where each image's key is, an `input_image` (a base64 data URL) in
//!   its place, as Jev reads a key (`super::pieces`). A question's instructions take parts too.
//! - Jev's questions, keyed by id, are a list each `name`d by its id. `noul` is `predicate`,
//!   which has no criteria, so a noul's `true` and `false` go into its instructions as words.
//!   `choice` keeps its type, its options `choices` of `{value, description}` (a card goes as its
//!   JSON). `score`'s levels are `{label}`, lowest first, its answer the probability-weighted
//!   index as Jev's is.
//! - Answers come back as a list in the questions' order, each with its `name`: a predicate's
//!   `probability` is Jev's `noul`, a choice's and a score's `probabilities` (a list of `{value,
//!   probability}`) become Jev's map. A `refusal` answers nothing, as a question Jev left out.
//! - `usage.input_tokens` counts cached and cache-write tokens too
//!   (`input_tokens_details.cached_tokens`, `cache_write_tokens`); output isn't charged.
//! - Errors are `{"error": {"message", "type", "code"}}`: 401 a bad key, 429 a rate limit, 5xx
//!   (503 `server_is_overloaded`) for now.

use anyhow::Result;
use serde_json::{json, Map, Value};

use super::{asks, mime, pieces, text_of, Adapter, Piece, Tokens};

pub struct OpenAi;

/// Dollars per million tokens: input not cached, cached, and written to the cache (its 30
/// minutes); the Decisions guide says output isn't charged.
const INPUT: f64 = 0.10;
const CACHED: f64 = 0.01;
const CACHE_WRITE: f64 = 0.125;
/// The most `input_json` takes, written out compactly.
const JSON_MOST: usize = 256 * 1024;

impl Adapter for OpenAi {
    fn id(&self) -> &'static str {
        "openai"
    }

    fn word(&self) -> &'static str {
        "OpenAI"
    }

    fn keychain_service(&self) -> &'static str {
        "Codealong OpenAI"
    }

    fn key_env(&self) -> &'static str {
        "OPENAI_API_KEY"
    }

    fn endpoint(&self) -> &'static str {
        "https://api.openai.com/v1/decisions"
    }

    fn headers(&self, key: &str, _images: bool) -> Vec<String> {
        vec![format!("Authorization: Bearer {key}")]
    }

    fn request(&self, model: &str, body: &Value) -> Value {
        let assets = body["assets"].as_object().cloned().unwrap_or_default();
        let questions: Vec<Value> = body["questions"].as_object().into_iter().flatten().filter(|(_, q)| asks(q)).filter_map(|(id, q)| question(id, q, &assets)).collect();
        json!({
            "model": model,
            "input": [{ "role": "user", "content": content(&body["state"], &assets) }],
            "questions": questions,
        })
    }

    fn answer(&self, model: &str, reply: Value) -> Result<Value> {
        let mut answers = Map::new();
        for a in reply["answers"].as_array().into_iter().flatten() {
            let Some(name) = a["name"].as_str() else { continue };
            let probabilities = || -> Map<String, Value> {
                a["probabilities"].as_array().into_iter().flatten().filter_map(|p| Some((option(&p["value"])?, p["probability"].clone()))).collect()
            };
            let answer = match a["type"].as_str() {
                Some("predicate") => json!({ "type": "noul", "noul": a["probability"] }),
                Some("choice") => json!({ "type": "choice", "choice": option(&a["choice"]), "probabilities": probabilities(), "confidence": a["confidence"] }),
                Some("score") => json!({ "type": "score", "score": a["score"], "probabilities": probabilities(), "confidence": a["confidence"] }),
                _ => continue,
            };
            answers.insert(name.to_string(), answer);
        }
        let usage = &reply["usage"];
        let details = &usage["input_tokens_details"];
        Ok(json!({
            "model": reply["model"].as_str().unwrap_or(model),
            "answers": answers,
            "usage": {
                "input_tokens": usage["input_tokens"].as_u64().unwrap_or(0),
                "output_tokens": usage["output_tokens"].as_u64().unwrap_or(0),
                "cached_tokens": details["cached_tokens"].as_u64().unwrap_or(0),
                "cache_write_tokens": details["cache_write_tokens"].as_u64().unwrap_or(0),
            },
        }))
    }

    fn cost(&self, used: &Tokens) -> f64 {
        let fresh = used.input.saturating_sub(used.cached + used.cache_write);
        (fresh as f64 * INPUT + used.cached as f64 * CACHED + used.cache_write as f64 * CACHE_WRITE) / 1_000_000.0
    }
}

/// An option's value as Jev names it: a choice's string (or a boolean), a score's level index,
/// written out.
fn option(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// The state as a message's content: JSON as JSON while it has no images and fits, else as text
/// with each image in its key's place.
fn content(state: &Value, assets: &Map<String, Value>) -> Value {
    let text = match state {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let is_json = state.is_object() || state.as_array().is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_object));
    if assets.is_empty() && is_json && text.len() <= JSON_MOST {
        return json!([{ "type": "input_json", "data": state }]);
    }
    Value::Array(parts(&text, assets))
}

fn parts(text: &str, assets: &Map<String, Value>) -> Vec<Value> {
    pieces(text, assets)
        .into_iter()
        .map(|p| match p {
            Piece::Text(t) => json!({ "type": "input_text", "text": t }),
            Piece::Image { format, data } => json!({ "type": "input_image", "image_url": format!("data:{};base64,{data}", mime(&format)) }),
        })
        .collect()
}

/// One of Jev's questions as OpenAI's, named by its id. None for a type it has no match for
/// (Jev's `box`), which Codealong doesn't ask.
fn question(id: &str, q: &Value, assets: &Map<String, Value>) -> Option<Value> {
    let mut instructions = text_of(&q["instructions"]);
    let criteria = &q["criteria"];
    let mut out = match q["type"].as_str()? {
        "noul" => {
            for (side, says) in [("true", "Answer yes when"), ("false", "Answer no when")] {
                if let Some(c) = criteria.get(side).map(text_of).filter(|c| !c.trim().is_empty()) {
                    instructions.push_str(&format!("\n{says}: {c}"));
                }
            }
            json!({ "type": "predicate" })
        }
        "choice" => {
            let choices: Vec<Value> = criteria
                .as_object()
                .into_iter()
                .flatten()
                .map(|(value, d)| match text_of(d) {
                    d if d.trim().is_empty() => json!({ "value": value }),
                    d => json!({ "value": value, "description": d }),
                })
                .collect();
            json!({ "type": "choice", "choices": choices })
        }
        "score" => {
            let levels: Vec<Value> = criteria.as_array().into_iter().flatten().map(|l| json!({ "label": text_of(l) })).collect();
            json!({ "type": "score", "levels": levels })
        }
        _ => return None,
    };
    out["name"] = json!(id);
    out["instructions"] = if assets.keys().any(|k| instructions.contains(k.as_str())) { Value::Array(parts(&instructions, assets)) } else { json!(instructions) };
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body() -> Value {
        crate::jev::body(
            "jev",
            json!({ "message": "fix it", "sessions": { "s1": { "title": "Atlas billing" } } }),
            json!({
                "action": { "type": "choice", "instructions": "What should happen with `message`?", "criteria": { "send": "Send it.", "s1": { "title": "Atlas billing" }, "none": "" } },
                "own_worktree": { "type": "noul", "instructions": "A new worktree?", "criteria": { "true": "It says so.", "false": "It doesn't." } },
                "risk": { "type": "score", "instructions": "How risky?", "criteria": ["Reads", "Writes"] },
                "where": { "type": "box", "instructions": "Where?" },
                "duplicate": { "type": "choice", "instructions": "Which?", "criteria": { "none": "None of these." } },
                "c1": { "type": "noul", "instructions": { "comment": { "by": "ana", "text": "Rename this" }, "question": "Does `comment` ask for a change?" } },
            }),
        )
    }

    #[test]
    fn jevs_questions_go_as_openais() {
        let sent = OpenAi.request("gpt-6-luna", &body());
        assert_eq!(sent["model"], "gpt-6-luna");
        assert_eq!(sent["input"], json!([{ "role": "user", "content": [{ "type": "input_json", "data": body()["state"] }] }]));
        let q = |name: &str| sent["questions"].as_array().unwrap().iter().find(|q| q["name"] == name).cloned();
        assert_eq!(
            q("action"),
            Some(json!({ "type": "choice", "name": "action", "instructions": "What should happen with `message`?", "choices": [
                { "value": "none" },
                { "value": "s1", "description": "{\"title\":\"Atlas billing\"}" },
                { "value": "send", "description": "Send it." },
            ] }))
        );
        assert_eq!(q("own_worktree"), Some(json!({ "type": "predicate", "name": "own_worktree", "instructions": "A new worktree?\nAnswer yes when: It says so.\nAnswer no when: It doesn't." })));
        assert_eq!(q("risk"), Some(json!({ "type": "score", "name": "risk", "instructions": "How risky?", "levels": [{ "label": "Reads" }, { "label": "Writes" }] })));
        assert_eq!(q("where"), None, "no box");
        assert_eq!(q("duplicate"), None, "a choice of one isn't asked: it has its answer");
        assert_eq!(q("c1").map(|c| c["instructions"].clone()), Some(json!("{\"comment\":{\"by\":\"ana\",\"text\":\"Rename this\"},\"question\":\"Does `comment` ask for a change?\"}")), "a card in the instructions goes as its JSON");
    }

    #[test]
    fn images_go_where_their_keys_are() {
        let mut b = crate::jev::body("jev", json!({ "message": "fix image-aa now" }), json!({}));
        b["assets"] = json!({ "image-aa": { "type": "image", "format": "jpg", "data_base64": "AAAA" } });
        let sent = OpenAi.request("gpt-6-luna", &b);
        assert_eq!(
            sent["input"][0]["content"],
            json!([
                { "type": "input_text", "text": "{\"message\":\"fix " },
                { "type": "input_image", "image_url": "data:image/jpeg;base64,AAAA" },
                { "type": "input_text", "text": " now\"}" },
            ])
        );
        // A string state goes as text.
        assert_eq!(OpenAi.request("m", &crate::jev::body("jev", json!("Fix the flaky login test"), json!({})))["input"][0]["content"], json!([{ "type": "input_text", "text": "Fix the flaky login test" }]));
    }

    #[test]
    fn openais_answers_come_back_as_jevs() {
        // As the Decisions guide shows them.
        let reply = json!({
            "model": "gpt-6-luna",
            "answers": [
                { "type": "choice", "name": "action", "choice": "send", "probabilities": [{ "value": "send", "probability": 0.8 }, { "value": "none", "probability": 0.2 }], "confidence": 0.7 },
                { "type": "predicate", "name": "own_worktree", "probability": 0.1 },
                { "type": "score", "name": "risk", "score": 0.4, "probabilities": [{ "value": 0, "label": "Reads", "probability": 0.6 }, { "value": 1, "label": "Writes", "probability": 0.4 }], "confidence": 0.5 },
                { "type": "refusal", "name": "where" },
            ],
            "usage": { "input_tokens": 2000, "input_tokens_details": { "cached_tokens": 1024, "cache_write_tokens": 0 }, "output_tokens": 12 },
        });
        let v = OpenAi.answer("gpt-6-luna", reply).unwrap();
        assert_eq!(crate::jev::choice(&v, "action"), vec![("send".to_string(), 0.8), ("none".to_string(), 0.2)]);
        assert_eq!((crate::jev::confidence(&v, "action"), crate::jev::noul(&v, "own_worktree")), (Some(0.7), Some(0.1)));
        assert_eq!((crate::jev::score(&v, "risk"), v["answers"]["risk"]["probabilities"]["1"].as_f64()), (Some(0.4), Some(0.4)));
        assert!(v["answers"].get("where").is_none(), "a refusal answers nothing");
        assert_eq!((v["model"].as_str(), v["usage"]["input_tokens"].as_u64(), v["usage"]["cached_tokens"].as_u64()), (Some("gpt-6-luna"), Some(2000), Some(1024)));
        let cost = OpenAi.cost(&Tokens { input: 2_000_000, cached: 1_000_000, cache_write: 0, output: 50 });
        assert!((cost - 0.11).abs() < 1e-12, "{cost}");
    }
}
