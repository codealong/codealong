//! Whether OpenAI may train its models on a Codex account's conversations and tasks: "Improve
//! the model for everyone" in ChatGPT's Data Controls, which is `training_allowed` in the
//! account's settings. Codealong reads it with Codex's own sign-in (`auth.json`) as the ChatGPT
//! and Codex apps do (ChatGPT 26.930, Codex 26.831: `GET chatgpt.com/backend-api/settings/user`)
//! and changes it as they do (`PATCH settings/account_user_setting?feature=training_allowed&
//! value=false`, no body, answered with the settings it set). Beside it are Codex's own two,
//! `codex_training_allowed` and `codex_training_allowed_v2`, read and turned off the same way;
//! no app was seen sending those, so they're sent only when on. Business, Enterprise and Edu
//! workspaces aren't trained on. An API key's calls aren't either unless its organization shares
//! them, which only the API's dashboard says. Codealong never refreshes the sign-in: Codex does
//! as it runs.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use serde_json::Value;

use crate::config::AccountCfg;
use crate::training::{State, Training};
use crate::util::code;

const SETTINGS: &str = "https://chatgpt.com/backend-api/settings";
/// Where a person changes it by hand.
const PAGE: &str = "https://chatgpt.com/#settings/DataControls";
/// The settings that let OpenAI train on the account: ChatGPT's, then Codex's two.
const SWITCHES: [&str; 3] = ["training_allowed", "codex_training_allowed", "codex_training_allowed_v2"];

struct SignIn {
    token: String,
    account_id: Option<String>,
    /// `plus`, `pro`, `business`, `enterprise`… from the token's claims.
    plan: Option<String>,
}

enum Auth {
    ChatGpt(SignIn),
    ApiKey,
}

fn read_auth(home: &Path) -> Option<Auth> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(home.join("auth.json")).ok()?).ok()?;
    let tokens = v.get("tokens").filter(|t| t.is_object());
    let token = tokens.and_then(|t| t.get("access_token")).and_then(Value::as_str).filter(|t| !t.is_empty());
    match token {
        Some(token) => Some(Auth::ChatGpt(SignIn {
            token: token.to_string(),
            account_id: tokens.and_then(|t| t.get("account_id")).and_then(Value::as_str).map(str::to_string),
            plan: plan_of(token),
        })),
        None if v.get("OPENAI_API_KEY").and_then(Value::as_str).is_some_and(|k| !k.is_empty()) => Some(Auth::ApiKey),
        None => None,
    }
}

/// The plan a ChatGPT token's claims name (`https://api.openai.com/auth`.`chatgpt_plan_type`).
fn plan_of(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let bytes = data_encoding::BASE64URL_NOPAD.decode(payload.trim_end_matches('=').as_bytes()).ok()?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    claims.pointer("/https:~1~1api.openai.com~1auth/chatgpt_plan_type").and_then(Value::as_str).map(str::to_string)
}

/// Workspace plans, which OpenAI doesn't train on. Enterprise can read as `business`.
fn workspace(plan: &str) -> bool {
    matches!(plan, "team" | "business" | "enterprise" | "edu")
}

fn call(sign_in: &SignIn, method: &str, url: &str) -> Result<Value, String> {
    let mut headers = vec![format!("Authorization: Bearer {}", sign_in.token)];
    headers.extend(sign_in.account_id.iter().map(|id| format!("ChatGPT-Account-Id: {id}")));
    let reply = crate::http::call(method, url, &headers, None, Duration::from_secs(15))?;
    match reply.status {
        200..=299 => reply.json().ok_or_else(|| "unexpected response".to_string()),
        401 => Err("signed out".to_string()),
        other => Err(format!("HTTP {other}")),
    }
}

pub fn read(account: &AccountCfg) -> Training {
    let fix = format!("run {} once to sign in again", code(account.command()));
    let sign_in = match read_auth(&account.home_path()) {
        None => return Training::unknown(account, format!("Not signed in: {fix}.")).at(PAGE),
        Some(Auth::ApiKey) => return Training::unknown(account, "Signed in with an API key: OpenAI doesn't train on API calls unless your organization opts in to share them, which Codealong can't read."),
        Some(Auth::ChatGpt(s)) => s,
    };
    if let Some(plan) = sign_in.plan.as_deref().filter(|p| workspace(p)) {
        return Training::new(account, State::Off, format!("ChatGPT {} workspaces aren't trained on.", crate::usage::title(plan)));
    }
    match call(&sign_in, "GET", &format!("{SETTINGS}/user")) {
        Ok(v) => from_settings(account, &v),
        Err(why) if why == "signed out" => Training::unknown(account, format!("Sign-in expired: {fix}.")).at(PAGE),
        Err(why) => Training::unknown(account, format!("Couldn't read its data controls: {why}.")).at(PAGE),
    }
}

/// Where the account stands from `settings/user`'s answer, whose `settings` holds the switches.
pub fn from_settings(account: &AccountCfg, v: &Value) -> Training {
    let Some(settings) = v.get("settings").filter(|s| s.is_object()) else {
        return Training::unknown(account, "Couldn't read its data controls: unexpected response.").at(PAGE);
    };
    let on = |name: &str| settings.get(name).and_then(Value::as_bool) == Some(true);
    if on(SWITCHES[0]) {
        Training::new(account, State::On, "Improve the model for everyone is on in ChatGPT's data controls: OpenAI may train its models on this account's conversations and Codex tasks.").at(PAGE).stoppable()
    } else if SWITCHES[1..].iter().any(|s| on(s)) {
        Training::new(account, State::On, "Codex's training setting is on: OpenAI may train its models on this account's Codex tasks.").at(PAGE).stoppable()
    } else {
        Training::new(account, State::Off, "Improve the model for everyone is off in ChatGPT's data controls.").at(PAGE)
    }
}

/// Turns off every switch that's on, as the apps' settings do, one call each.
pub fn stop(account: &AccountCfg) -> Result<()> {
    let sign_in = match read_auth(&account.home_path()) {
        Some(Auth::ChatGpt(s)) => s,
        _ => bail!("{} isn't signed in to ChatGPT: run {} once to sign in.", account.name, code(account.command())),
    };
    let now = call(&sign_in, "GET", &format!("{SETTINGS}/user")).map_err(|why| anyhow!("Couldn't read {}'s data controls: {why}.", account.name))?;
    let on = |name: &str| now.pointer(&format!("/settings/{name}")).and_then(Value::as_bool) == Some(true);
    // ChatGPT's own switch is always sent: it's the one its settings page shows.
    for name in SWITCHES.iter().filter(|s| **s == SWITCHES[0] || on(s)) {
        call(&sign_in, "PATCH", &format!("{SETTINGS}/account_user_setting?feature={name}&value=false")).map_err(|why| anyhow!("Couldn't turn off {name} for {}: {why}.", account.name))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn account() -> AccountCfg {
        AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: None, start: None, preferred: false, acp: None, monthly: None, per_token: false }
    }

    /// As `settings/user` answered on 2026-10-13, with the rest of the settings left out.
    #[test]
    fn reads_improve_the_model_for_everyone() {
        let read = |v: Value| from_settings(&account(), &v);
        let off = read(json!({"settings": {"training_allowed": false, "codex_training_allowed": false, "codex_training_allowed_v2": false}}));
        assert_eq!((off.state, off.can_stop), (State::Off, false));
        let on = read(json!({"settings": {"training_allowed": true}}));
        assert_eq!((on.state, on.can_stop, on.url.as_deref()), (State::On, true, Some(PAGE)));
        assert_eq!(read(json!({"settings": {"training_allowed": false, "codex_training_allowed_v2": true}})).state, State::On);
        assert_eq!(read(json!({"detail": "no"})).state, State::Unknown);
    }

    #[test]
    fn reads_the_plan_from_the_tokens_claims() {
        let claims = data_encoding::BASE64URL_NOPAD.encode(br#"{"https://api.openai.com/auth":{"chatgpt_plan_type":"business"}}"#);
        assert_eq!(plan_of(&format!("e30.{claims}.sig")).as_deref(), Some("business"));
        assert_eq!(plan_of("not a token"), None);
        assert!(workspace("business") && workspace("enterprise") && !workspace("pro") && !workspace("plus"));
    }
}
