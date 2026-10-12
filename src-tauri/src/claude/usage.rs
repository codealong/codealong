//! A Claude account's plan usage: its own sign-in, read from the login keychain the way Claude
//! Code reads it, sent to the endpoint behind Claude Code's `/usage`. Codealong never refreshes
//! or writes a sign-in: refreshing rotates the token and would sign Claude Code out. An expired
//! one shows as expired until the account's command runs again. The token goes to curl on
//! stdin, never on a command line, and is never logged.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::config::AccountCfg;
use crate::model::{Meter, Usage};
use crate::procs;
use crate::usage::{loading, severity, title};
use crate::util::{code, home, parse_rfc3339_ms};

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
/// A locked keychain can hold `security` at a prompt; don't wait on it forever.
const KEYCHAIN_TIMEOUT: Duration = Duration::from_secs(10);

/// The keychain services Claude Code keeps a config folder's sign-in under: plain for the
/// default `~/.claude`, and suffixed with a hash of the folder when `CLAUDE_CONFIG_DIR` names it.
pub fn keychain_services(config_dir: &Path) -> Vec<String> {
    let dir = config_dir.to_string_lossy();
    let hashed = |p: &str| format!("Claude Code-credentials-{}", &format!("{:x}", Sha256::digest(p.as_bytes()))[..8]);
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

/// True when the keychain holds a sign-in for this config folder. Reads no secret.
pub fn claude_signed_in(config_dir: &Path) -> bool {
    let account = keychain_account();
    keychain_services(config_dir)
        .iter()
        .any(|svc| procs::run("/usr/bin/security", &["find-generic-password", "-a", &account, "-s", svc], None, KEYCHAIN_TIMEOUT).is_ok())
}

struct SignIn {
    token: String,
    expires_at: Option<i64>,
    plan: Option<String>,
}

fn read_sign_in(config_dir: &Path) -> Option<SignIn> {
    let account = keychain_account();
    for svc in keychain_services(config_dir) {
        // Fails on a missing item (exit 44); stdout, the secret, is never part of an error.
        let Ok(out) = procs::run("/usr/bin/security", &["find-generic-password", "-a", &account, "-w", "-s", &svc], None, KEYCHAIN_TIMEOUT) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<Value>(out.trim()) else { continue };
        let Some(oauth) = v.get("claudeAiOauth") else { continue };
        let Some(token) = oauth.get("accessToken").and_then(Value::as_str).filter(|t| !t.is_empty()) else { continue };
        return Some(SignIn {
            token: token.to_string(),
            expires_at: oauth.get("expiresAt").and_then(Value::as_i64),
            plan: claude_plan(oauth.get("subscriptionType").and_then(Value::as_str), oauth.get("rateLimitTier").and_then(Value::as_str)),
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
    match fetch(&sign_in.token) {
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

enum Fetch {
    SignedOut,
    RateLimited,
    Failed(String),
}

fn fetch(token: &str) -> Result<Value, Fetch> {
    if token.chars().any(|c| c == '"' || c == '\\' || c.is_control()) {
        return Err(Fetch::Failed("the stored sign-in is unreadable".into()));
    }
    let mut child = Command::new("/usr/bin/curl")
        .args(["-sS", "-m", "15", "-w", "\n%{http_code}", "-K", "-", USAGE_URL])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Fetch::Failed(format!("curl: {e}")))?;
    let config = format!(
        "header = \"Authorization: Bearer {token}\"\nheader = \"anthropic-beta: oauth-2025-04-20\"\nheader = \"Accept: application/json\"\nheader = \"User-Agent: codealong/{}\"\n",
        env!("CARGO_PKG_VERSION")
    );
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(config.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| Fetch::Failed(format!("curl: {e}")))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let (body, status) = text.rsplit_once('\n').unwrap_or(("", &text));
    match status.trim() {
        "200" => serde_json::from_str(body).map_err(|_| Fetch::Failed("unexpected response".into())),
        "401" | "403" => Err(Fetch::SignedOut),
        "429" => Err(Fetch::RateLimited),
        "000" | "" => {
            let err = String::from_utf8_lossy(&out.stderr);
            Err(Fetch::Failed(err.trim().trim_start_matches("curl: ").chars().take(120).collect()))
        }
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

    #[test]
    fn keychain_service_for_a_second_config_folder() {
        let services = keychain_services(Path::new("/Users/ghempton/.claude2"));
        assert_eq!(services[0], "Claude Code-credentials-71eb27f7");
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
