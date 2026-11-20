//! TypeSafe's Jev (docs.typesafe.ai, and .claude/rules/jev.md for what was learned of it): the
//! request Codealong writes is Jev's own (jev.rs `body`), so nothing is translated either way.
//! Images go as the body's assets, named in the text by their keys, with
//! `TypeSafe-Danger-Zone-Images: true`, which TypeSafe refuses a request with images without.

use anyhow::Result;
use serde_json::Value;

use super::{Adapter, Tokens};

pub struct TypeSafe;

/// Dollars per million input tokens, on every Jev model; output tokens are free.
const PRICE: f64 = 0.042;

impl Adapter for TypeSafe {
    fn id(&self) -> &'static str {
        "typesafe"
    }

    fn word(&self) -> &'static str {
        "TypeSafe"
    }

    fn keychain_service(&self) -> &'static str {
        "Codealong TypeSafe"
    }

    fn key_env(&self) -> &'static str {
        "TYPESAFE_API_KEY"
    }

    fn endpoint(&self) -> &'static str {
        "https://api.typesafe.ai/v1/systemone"
    }

    fn headers(&self, key: &str, images: bool) -> Vec<String> {
        let mut out = vec![format!("Authorization: Bearer {key}")];
        if images {
            out.push("TypeSafe-Danger-Zone-Images: true".into());
        }
        out
    }

    fn request(&self, model: &str, body: &Value) -> Value {
        let mut out = body.clone();
        out["model"] = Value::from(model);
        out
    }

    fn answer(&self, _model: &str, reply: Value) -> Result<Value> {
        Ok(reply)
    }

    fn cost(&self, used: &Tokens) -> f64 {
        used.input as f64 * PRICE / 1_000_000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn jevs_request_goes_as_it_is_on_the_model_asked() {
        let body = crate::jev::body("jev", json!({ "message": "hi" }), json!({ "q": { "type": "noul", "instructions": "?" } }));
        let sent = TypeSafe.request("jev-1.14.0", &body);
        assert_eq!((sent["model"].as_str(), &sent["state"], &sent["questions"]), (Some("jev-1.14.0"), &body["state"], &body["questions"]));
        assert!((TypeSafe.cost(&Tokens { input: 1_000_000, ..Default::default() }) - 0.042).abs() < 1e-12);
    }
}
