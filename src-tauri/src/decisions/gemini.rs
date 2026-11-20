//! Google's Gemini Decision 1 (aistudio.google.com/docs/decisions-api and
//! /api-reference/decisions, read 2026-11-19): `POST v1beta/decisions` with `model`, `input` and
//! `questions`, the key in `x-goog-api-key`, on `gemini-decision-1`.
//!
//! - Its questions are Jev's: a map by id of `noul`, `choice` and `score`, each with
//!   `instructions` and `criteria`, which is a choice's options by id with their descriptions (a
//!   card goes as its JSON), a score's levels lowest first, and for a noul `{true_description,
//!   false_description}` in place of Jev's `{true, false}`. Instructions and criteria are text
//!   only, so an image key there is taken out.
//! - `input` takes text and media blocks, no JSON: the state goes as its JSON in a text block,
//!   cut where each image's key is, an image block (base64 `data` and its `mime_type`) in its
//!   place (`super::pieces`).
//! - Answers are Jev's, by id, with one difference: a noul's carries a `confidence` (the likelier
//!   side's chance), which Jev's doesn't, and it's left out so a read's least sure answer
//!   (insights.rs) is taken from the same questions whoever answered. A choice's `confidence` is
//!   its likeliest option's probability, where Jev's is its own measure.
//! - A blocked input answers 200 with `prompt_feedback` set and no answers (the guide says 400),
//!   taken as a request it couldn't take. No `model` comes back.
//! - `usage.total_input_tokens` is all it charges for ($0.10 a million on the paid tier, nothing
//!   on the free one, which Google may learn from); no caching. Limits are 429
//!   `RESOURCE_EXHAUSTED`.

use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::{described, mime, pieces, Adapter, Piece, Tokens};

pub struct Gemini;

/// Dollars per million input tokens on the paid tier; output is free.
const PRICE: f64 = 0.10;

impl Adapter for Gemini {
    fn id(&self) -> &'static str {
        "gemini"
    }

    fn word(&self) -> &'static str {
        "Gemini"
    }

    fn keychain_service(&self) -> &'static str {
        "Codealong Gemini"
    }

    fn key_env(&self) -> &'static str {
        "GEMINI_API_KEY"
    }

    fn endpoint(&self) -> &'static str {
        "https://generativelanguage.googleapis.com/v1beta/decisions"
    }

    fn headers(&self, key: &str, _images: bool) -> Vec<String> {
        vec![format!("x-goog-api-key: {key}")]
    }

    fn request(&self, model: &str, body: &Value) -> Value {
        let assets = body["assets"].as_object().cloned().unwrap_or_default();
        let state = match &body["state"] {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let input: Vec<Value> = pieces(&state, &assets)
            .into_iter()
            .map(|p| match p {
                Piece::Text(t) => json!({ "type": "text", "text": t }),
                Piece::Image { format, data } => json!({ "type": "image", "data": data, "mime_type": mime(&format) }),
            })
            .collect();
        let keys: Vec<String> = assets.keys().cloned().collect();
        let questions: Map<String, Value> = body["questions"].as_object().into_iter().flatten().filter_map(|(id, q)| Some((id.clone(), question(q, &keys)?))).collect();
        json!({ "model": model, "input": input, "questions": questions })
    }

    fn answer(&self, model: &str, reply: Value) -> Result<Value> {
        let answers = reply["answers"].as_object().cloned().unwrap_or_default();
        if answers.is_empty() {
            if let Some(why) = reply.get("prompt_feedback").filter(|f| !f.is_null()) {
                let reason = why["block_reason"].as_str().unwrap_or("blocked");
                bail!("Gemini couldn't take the request: its input was blocked ({reason}).");
            }
        }
        let answers: Map<String, Value> = answers
            .into_iter()
            .map(|(id, mut a)| {
                if a["type"] == "noul" {
                    if let Some(o) = a.as_object_mut() {
                        o.remove("confidence");
                    }
                }
                (id, a)
            })
            .collect();
        let usage = &reply["usage"];
        Ok(json!({
            "model": reply["model"].as_str().unwrap_or(model),
            "answers": answers,
            "usage": {
                "input_tokens": usage["total_input_tokens"].as_u64().unwrap_or(0),
                "output_tokens": usage["total_output_tokens"].as_u64().unwrap_or(0),
                "cached_tokens": usage["total_cached_tokens"].as_u64().unwrap_or(0),
            },
        }))
    }

    fn cost(&self, used: &Tokens) -> f64 {
        used.input as f64 * PRICE / 1_000_000.0
    }
}

/// Text with the image keys in it taken out, where only text goes.
fn plain(text: &str, keys: &[String]) -> String {
    keys.iter().fold(text.to_string(), |t, k| t.replace(k.as_str(), ""))
}

/// One of Jev's questions as Gemini's. None for a type it has no match for (Jev's `box`), which
/// Codealong doesn't ask.
fn question(q: &Value, keys: &[String]) -> Option<Value> {
    let kind = q["type"].as_str()?;
    let instructions = plain(q["instructions"].as_str().unwrap_or_default(), keys);
    let criteria = &q["criteria"];
    let text = |v: &Value| plain(&described(v), keys);
    let criteria = match kind {
        "noul" => {
            let mut sides = Map::new();
            for (side, as_) in [("true", "true_description"), ("false", "false_description")] {
                if let Some(c) = criteria.get(side) {
                    sides.insert(as_.into(), json!(text(c)));
                }
            }
            (!sides.is_empty()).then_some(Value::Object(sides))
        }
        "choice" => Some(Value::Object(criteria.as_object().into_iter().flatten().map(|(id, d)| (id.clone(), json!(text(d)))).collect())),
        "score" => Some(Value::Array(criteria.as_array().into_iter().flatten().map(|l| json!(text(l))).collect())),
        _ => return None,
    };
    let mut out = json!({ "type": kind, "instructions": instructions });
    if let Some(c) = criteria {
        out["criteria"] = c;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jevs_request_goes_as_geminis() {
        let mut b = crate::jev::body(
            "jev",
            json!({ "message": "fix image-aa" }),
            json!({
                "action": { "type": "choice", "instructions": "What should happen with `message`?", "criteria": { "send": "Send it.", "s1": { "title": "Atlas billing" } } },
                "own_worktree": { "type": "noul", "instructions": "A new worktree?", "criteria": { "true": "It says so.", "false": "It doesn't." } },
                "finished": { "type": "noul", "instructions": "Finished, image-aa?" },
                "risk": { "type": "score", "instructions": "How risky?", "criteria": ["Reads", "Writes"] },
                "where": { "type": "box", "instructions": "Where?" },
            }),
        );
        b["assets"] = json!({ "image-aa": { "type": "image", "format": "png", "data_base64": "AAAA" } });
        let sent = Gemini.request("gemini-decision-1", &b);
        assert_eq!(sent["model"], "gemini-decision-1");
        assert_eq!(sent["input"], json!([{ "type": "text", "text": "{\"message\":\"fix " }, { "type": "image", "data": "AAAA", "mime_type": "image/png" }, { "type": "text", "text": "\"}" }]));
        let q = &sent["questions"];
        assert_eq!(q["action"], json!({ "type": "choice", "instructions": "What should happen with `message`?", "criteria": { "send": "Send it.", "s1": "{\"title\":\"Atlas billing\"}" } }));
        assert_eq!(q["own_worktree"]["criteria"], json!({ "true_description": "It says so.", "false_description": "It doesn't." }));
        assert_eq!(q["finished"], json!({ "type": "noul", "instructions": "Finished, ?" }), "text only, and no criteria when Jev's had none");
        assert_eq!(q["risk"]["criteria"], json!(["Reads", "Writes"]));
        assert!(q.get("where").is_none());
    }

    #[test]
    fn geminis_answers_come_back_as_jevs() {
        let reply = json!({
            "answers": {
                "action": { "type": "choice", "choice": "send", "probabilities": { "send": 0.9, "s1": 0.1 }, "confidence": 0.9 },
                "own_worktree": { "type": "noul", "noul": 0.2, "confidence": 0.8 },
                "risk": { "type": "score", "score": 1.2, "legend": { "0": "Reads", "1": "Writes" }, "probabilities": { "0": 0.3, "1": 0.7 }, "confidence": 0.7 },
            },
            "usage": { "total_input_tokens": 900, "total_output_tokens": 0, "total_tokens": 900 },
        });
        let v = Gemini.answer("gemini-decision-1", reply).unwrap();
        assert_eq!(crate::jev::choice(&v, "action")[0], ("send".to_string(), 0.9));
        assert_eq!((crate::jev::noul(&v, "own_worktree"), crate::jev::confidence(&v, "own_worktree")), (Some(0.2), None));
        assert_eq!(crate::jev::score(&v, "risk"), Some(1.2));
        assert_eq!((v["model"].as_str(), v["usage"]["input_tokens"].as_u64()), (Some("gemini-decision-1"), Some(900)));
        let blocked = Gemini.answer("gemini-decision-1", json!({ "answers": {}, "prompt_feedback": { "block_reason": "safety" } })).unwrap_err();
        assert_eq!(blocked.to_string(), "Gemini couldn't take the request: its input was blocked (safety).");
        assert!((Gemini.cost(&Tokens { input: 1_000_000, ..Default::default() }) - 0.10).abs() < 1e-12);
    }
}
