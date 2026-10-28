//! A Claude account's plan usage: its own sign-in, read from the login keychain the way Claude
//! Code reads it (on Linux from `.credentials.json` in its config folder, where Claude Code keeps
//! it there), sent to the endpoint behind Claude Code's `/usage`. Codealong never refreshes
//! or writes a sign-in: refreshing rotates the token and would sign Claude Code out. An expired
//! one shows as expired until the account's command runs again. The token goes to curl on
//! stdin (http.rs), never on a command line, and is never logged.

use std::path::Path;
use std::time::Duration;

use data_encoding::HEXLOWER;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::config::AccountCfg;
use crate::model::{Meter, Usage};
use crate::os::{self, Os};
use crate::usage::{loading, severity, title};
use crate::util::{code, home, parse_rfc3339_ms};

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
/// A locked keychain can hold `security` at a prompt; don't wait on it forever.
const KEYCHAIN_TIMEOUT: Duration = Duration::from_secs(10);

/// The keychain services Claude Code keeps a config folder's sign-in under: plain for the
/// default `~/.claude`, and suffixed with a hash of the folder when `CLAUDE_CONFIG_DIR` names it.
pub fn keychain_services(config_dir: &Path) -> Vec<String> {
    let dir = config_dir.to_string_lossy();
    let hashed = |p: &str| format!("Claude Code-credentials-{}", &HEXLOWER.encode(&Sha256::digest(p.as_bytes()))[..8]);
    let mut out = Vec::new();
    if config_dir == home().join(".claude") {
        out.push("Claude Code-credentials".to_string());
    }
    out.push(hashed(&dir));
    out.push(hashed(&format!("{}/", dir.trim_end_matches('/'))));
    out
}

/// The keychain account Claude Code files sign-ins under.
fn keychain_account() -> String {
    std::env::var("USER")
        .ok()
        .filter(|u| !u.is_empty() && u.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)))
        .unwrap_or_else(|| "claude-code-user".to_string())
}

/// True when Claude Code keeps a sign-in for this config folder: in the keychain on macOS, read
/// without the secret, and in the folder's `.credentials.json` on Linux.
pub fn claude_signed_in(config_dir: &Path) -> bool {
    match os::current() {
        Os::MacOs => {
            let account = keychain_account();
            keychain_services(config_dir).iter().any(|svc| os::secrets::keychain_has(svc, &account, KEYCHAIN_TIMEOUT))
        }
        Os::Linux => config_dir.join(".credentials.json").is_file(),
    }
}

/// The sign-ins kept for this config folder, as the JSON Claude Code keeps: on macOS the
/// keychain's items, the default's first, each read only once the one before it is found wanting,
/// since each can hold a prompt; on Linux the folder's `.credentials.json`.
fn kept_sign_ins(config_dir: &Path) -> Box<dyn Iterator<Item = String>> {
    match os::current() {
        Os::MacOs => {
            let account = keychain_account();
            Box::new(keychain_services(config_dir).into_iter().filter_map(move |svc| os::secrets::keychain_read(&svc, &account, KEYCHAIN_TIMEOUT)))
        }
        Os::Linux => Box::new(std::fs::read_to_string(config_dir.join(".credentials.json")).ok().into_iter()),
    }
}

pub(super) struct SignIn {
    pub token: String,
    pub expires_at: Option<i64>,
    pub plan: Option<String>,
    /// `pro`, `max`, `team`, `enterprise`: which terms the account is under.
    pub subscription: Option<String>,
}

pub(super) fn read_sign_in(config_dir: &Path) -> Option<SignIn> {
    for out in kept_sign_ins(config_dir) {
        let Ok(v) = serde_json::from_str::<Value>(out.trim()) else { continue };
        let Some(oauth) = v.get("claudeAiOauth") else { continue };
        let Some(token) = oauth.get("accessToken").and_then(Value::as_str).filter(|t| !t.is_empty()) else { continue };
        let subscription = oauth.get("subscriptionType").and_then(Value::as_str);
        return Some(SignIn {
            token: token.to_string(),
            expires_at: oauth.get("expiresAt").and_then(Value::as_i64),
            plan: claude_plan(subscription, oauth.get("rateLimitTier").and_then(Value::as_str)),
            subscription: subscription.map(str::to_string),
        });
    }
    None
}

/// `max` + `default_claude_max_20x` → `Max 20x`.
pub fn claude_plan(subscription: Option<&str>, tier: Option<&str>) -> Option<String> {
    let name = title(subscription?);
    let multiple = tier
        .and_then(|t| t.rsplit('_').next())
        .filter(|m| m.len() > 1 && m.ends_with('x') && m[..m.len() - 1].chars().all(|c| c.is_ascii_digit()));
    Some(match multiple {
        Some(m) => format!("{name} {m}"),
        None => name,
    })
}

/// Reads one Claude account's usage. Blocks for up to 15 seconds; call it off the engine thread.
pub fn claude(account: &AccountCfg, now: i64) -> Usage {
    let mut u = Usage { loading: false, ..loading(account) };
    let fix = format!("run {} once to sign in again", code(account.command()));
    let Some(sign_in) = read_sign_in(&account.home_path()) else {
        u.error = Some(format!("Not signed in: {fix}"));
        u.needs_sign_in = true;
        return u;
    };
    u.plan = sign_in.plan.clone();
    if sign_in.expires_at.map(|t| t <= now + 60_000).unwrap_or(false) {
        u.error = Some(format!("Sign-in expired: {fix}"));
        u.needs_sign_in = true;
        return u;
    }
    match request(&sign_in.token, "GET", USAGE_URL, None) {
        Ok(v) => {
            u.meters = claude_meters(&v);
            u.as_of = Some(now);
            if let Some((used, limit, currency)) = extra_spend(&v) {
                crate::ledger::note_extra(&account.name, now, used, limit, &currency);
            }
            if u.meters.is_empty() {
                u.error = Some("This plan doesn't report usage limits.".to_string());
            }
        }
        Err(Fetch::SignedOut) => {
            u.error = Some(format!("Sign-in expired: {fix}"));
            u.needs_sign_in = true;
        }
        Err(Fetch::RateLimited) => {
            u.error = Some("Couldn't read usage: rate limited, trying again in a few minutes".to_string());
            u.rate_limited = true;
        }
        Err(Fetch::Failed(why)) => u.error = Some(format!("Couldn't read usage: {why}")),
    }
    u
}

pub(super) enum Fetch {
    SignedOut,
    RateLimited,
    Failed(String),
}

/// One call to Anthropic's API as the account, with `body` as JSON when there is one: plan usage
/// here, its privacy settings in training.rs.
pub(super) fn request(token: &str, method: &str, url: &str, body: Option<&Value>) -> Result<Value, Fetch> {
    request_with(token, method, url, body, &["anthropic-beta: oauth-2025-04-20"])
}

/// `request` with headers of its own: another API's beta (mcp.rs's connectors).
pub(super) fn request_with(token: &str, method: &str, url: &str, body: Option<&Value>, extra: &[&str]) -> Result<Value, Fetch> {
    let headers: Vec<String> = std::iter::once(format!("Authorization: Bearer {token}")).chain(extra.iter().map(|h| h.to_string())).collect();
    let reply = crate::http::call(method, url, &headers, body, Duration::from_secs(15)).map_err(Fetch::Failed)?;
    match reply.status {
        200..=299 => reply.json().ok_or_else(|| Fetch::Failed("unexpected response".into())),
        401 | 403 => Err(Fetch::SignedOut),
        429 => Err(Fetch::RateLimited),
        other => Err(Fetch::Failed(format!("HTTP {other}"))),
    }
}

/// What extra usage has cost this billing period, and its monthly limit, in the currency's
/// whole units: the response's `extra_usage`, `{is_enabled, monthly_limit, used_credits,
/// utilization, currency, decimal_places, disabled_reason…}`, whose amounts are in minor units
/// (cents, two decimal places, as Claude Code 2.1.296's schema for it says). A plan with no
/// extra usage has none.
pub fn extra_spend(v: &Value) -> Option<(f64, Option<f64>, String)> {
    let e = v.get("extra_usage").filter(|e| e.is_object())?;
    let places = e.get("decimal_places").and_then(Value::as_i64).unwrap_or(2).clamp(0, 6) as i32;
    let unit = 10f64.powi(places);
    let used = e.get("used_credits").and_then(Value::as_f64)? / unit;
    let limit = e.get("monthly_limit").and_then(Value::as_f64).map(|l| l / unit);
    let currency = e.get("currency").and_then(Value::as_str).unwrap_or("USD").to_uppercase();
    Some((used, limit, currency))
}

/// The usage response's `limits` list: session, week and per-model weeks, in that order. Older
/// responses only have fixed windows; those are read when the list is missing.
pub fn claude_meters(v: &Value) -> Vec<Meter> {
    let mut out = Vec::new();
    for l in v.get("limits").and_then(Value::as_array).into_iter().flatten() {
        let kind = l.get("kind").and_then(Value::as_str).unwrap_or_default();
        let scope = l
            .pointer("/scope/model/display_name")
            .and_then(Value::as_str)
            .or_else(|| l.pointer("/scope/surface").and_then(Value::as_str))
            .map(title);
        let (key, label, window) = match (kind, scope) {
            ("session", _) => ("session".to_string(), "5h".to_string(), "5-hour session".to_string()),
            ("weekly_all", _) => ("week".to_string(), "Week".to_string(), "Weekly, all models".to_string()),
            (_, Some(name)) if kind.starts_with("weekly") => (format!("model:{name}"), name.clone(), format!("Weekly, {name} only")),
            _ => continue,
        };
        let percent = l.get("percent").and_then(Value::as_f64).unwrap_or(0.0);
        let model = key.strip_prefix("model:").map(str::to_string);
        out.push(Meter {
            key,
            label,
            window,
            percent,
            resets_at: l.get("resets_at").and_then(Value::as_str).and_then(parse_rfc3339_ms),
            severity: severity(l.get("severity").and_then(Value::as_str), percent),
            model,
        });
    }
    if !out.is_empty() {
        return out;
    }
    for (field, key, label, window) in [
        ("five_hour", "session", "5h", "5-hour session"),
        ("seven_day", "week", "Week", "Weekly, all models"),
        ("seven_day_opus", "model:Opus", "Opus", "Weekly, Opus only"),
        ("seven_day_sonnet", "model:Sonnet", "Sonnet", "Weekly, Sonnet only"),
    ] {
        let Some(w) = v.get(field).filter(|w| w.is_object()) else { continue };
        let percent = w.get("utilization").and_then(Value::as_f64).unwrap_or(0.0);
        out.push(Meter {
            key: key.to_string(),
            label: label.to_string(),
            window: window.to_string(),
            percent,
            resets_at: w.get("resets_at").and_then(Value::as_str).and_then(parse_rfc3339_ms),
            severity: severity(None, percent),
            model: key.strip_prefix("model:").map(str::to_string),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Severity;

    /// The first sign-in with a token is the one: the keychain isn't asked for the others.
    #[test]
    fn reads_the_keychain_only_until_a_sign_in_has_a_token() {
        let asked = std::rc::Rc::new(std::cell::Cell::new(0));
        let counted = asked.clone();
        crate::procs::stand_in::set(Some(Box::new(move |program, _, _, _| {
            assert_eq!(program, "/usr/bin/security");
            counted.set(counted.get() + 1);
            Ok(r#"{"claudeAiOauth":{"accessToken":"t","subscriptionType":"max"}}"#.to_string())
        })));
        let sign_in = os::as_if(Os::MacOs, || read_sign_in(&home().join(".claude")));
        crate::procs::stand_in::set(None);
        assert_eq!(sign_in.map(|s| s.token).as_deref(), Some("t"));
        assert_eq!(asked.get(), 1, "of {} items", keychain_services(&home().join(".claude")).len());
    }

    #[test]
    fn keychain_service_for_a_second_config_folder() {
        let services = keychain_services(Path::new("/Users/me/.claude2"));
        assert_eq!(services[0], "Claude Code-credentials-6e095c18");
        let default = keychain_services(&home().join(".claude"));
        assert_eq!(default[0], "Claude Code-credentials");
    }

    #[test]
    fn plan_names() {
        assert_eq!(claude_plan(Some("max"), Some("default_claude_max_20x")).as_deref(), Some("Max 20x"));
        assert_eq!(claude_plan(Some("pro"), Some("default_claude_pro")).as_deref(), Some("Pro"));
        assert_eq!(claude_plan(None, Some("default_claude_max_5x")), None);
    }

    #[test]
    fn reads_limits_with_per_model_weeks() {
        let v: Value = serde_json::from_str(
            r#"{"five_hour":{"utilization":22.0},"limits":[
              {"kind":"session","percent":22,"severity":"normal","resets_at":"2026-09-24T01:39:59.762949+00:00","scope":null},
              {"kind":"weekly_all","percent":47,"severity":"normal","resets_at":"2026-09-26T05:59:59.762968+00:00","scope":null},
              {"kind":"weekly_scoped","percent":79,"severity":"warning","resets_at":"2026-09-26T05:59:59.763150+00:00",
               "scope":{"model":{"id":null,"display_name":"Fable"},"surface":null}}]}"#,
        )
        .unwrap();
        let m = claude_meters(&v);
        let labels: Vec<&str> = m.iter().map(|m| m.label.as_str()).collect();
        assert_eq!(labels, ["5h", "Week", "Fable"]);
        assert_eq!(m[2].key, "model:Fable");
        assert_eq!(m[2].severity, Severity::Warning);
        assert_eq!(m[2].model.as_deref(), Some("Fable"));
        assert_eq!(m[0].resets_at, parse_rfc3339_ms("2026-09-24T01:39:59.762Z"));
    }

    #[test]
    fn falls_back_to_fixed_windows() {
        let v: Value = serde_json::from_str(r#"{"five_hour":{"utilization":95.0,"resets_at":null},"seven_day":{"utilization":10.0},"seven_day_opus":null}"#).unwrap();
        let m = claude_meters(&v);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].severity, Severity::Critical);
    }

    #[test]
    fn reads_extra_usage_in_dollars_from_its_cents() {
        // As the endpoint answered on 2026-10-12.
        let v: Value = serde_json::from_str(r#"{"extra_usage":{"is_enabled":false,"monthly_limit":10000,"used_credits":1234.0,"utilization":12.3,"currency":"USD","decimal_places":2,"disabled_reason":"out_of_credits"}}"#).unwrap();
        assert_eq!(extra_spend(&v), Some((12.34, Some(100.0), "USD".to_string())));
        assert_eq!(extra_spend(&serde_json::json!({"extra_usage": null})), None);
    }
}
