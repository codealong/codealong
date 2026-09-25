//! Plan usage per account. See *Harness adapters*, Usage.
//!
//! Claude: the account's own sign-in, read from the login keychain the way Claude Code reads
//! it, sent to the endpoint behind Claude Code's `/usage`. Codealong never refreshes or writes
//! a sign-in: refreshing rotates the token and would sign Claude Code out. An expired one shows
//! as expired until the account's command runs again. The token goes to curl on stdin, never
//! on a command line, and is never logged.
//!
//! Codex: no network and no credentials. Every turn records the account's rate limits in its
//! rollout, and the newest report wins.
//!
//! OpenCode and Antigravity: nothing to read. OpenCode bills whichever provider you bring, and
//! Antigravity keeps its quotas in the app.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::codex::RateLimit;
use crate::config::AccountCfg;
use crate::model::{AccountInfo, Harness, Meter, Severity, Usage};
use crate::procs;
use crate::util::{code, home, now_ms, parse_rfc3339_ms, DAY_MS};

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
/// A locked keychain can hold `security` at a prompt; don't wait on it forever.
const KEYCHAIN_TIMEOUT: Duration = Duration::from_secs(10);

pub fn loading(account: &AccountCfg) -> Usage {
    Usage {
        account: account.name.clone(),
        harness: Harness::from_config(&account.harness),
        plan: None,
        meters: Vec::new(),
        as_of: None,
        error: None,
        needs_sign_in: false,
        loading: true,
        rate_limited: false,
    }
}

/// An account whose harness reports no limits, with why in words.
pub fn none(account: &AccountCfg, note: &str) -> Usage {
    Usage { loading: false, error: Some(note.to_string()), ..loading(account) }
}

// ---- Claude ----

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

fn severity(word: Option<&str>, percent: f64) -> Severity {
    match word {
        Some("critical") => Severity::Critical,
        Some("warning") => Severity::Warning,
        Some(_) => Severity::Normal,
        None if percent >= 90.0 => Severity::Critical,
        None if percent >= 75.0 => Severity::Warning,
        None => Severity::Normal,
    }
}

// ---- Codex ----

/// Codex usage from the newest rate-limit reports across the account's threads. A window whose
/// reset time has passed since the report reads as empty. Per-model limits show only when
/// they've been used this week.
pub fn codex<'a>(account: &AccountCfg, reports: impl Iterator<Item = &'a BTreeMap<String, RateLimit>>, now: i64) -> Usage {
    let mut latest: BTreeMap<&str, &RateLimit> = BTreeMap::new();
    for map in reports {
        for (id, r) in map {
            if latest.get(id.as_str()).map(|old| old.at < r.at).unwrap_or(true) {
                latest.insert(id, r);
            }
        }
    }
    let mut u = Usage { loading: false, ..loading(account) };
    let Some(main) = latest.get("codex").copied().or_else(|| latest.values().find(|r| r.name.is_none()).copied()) else {
        u.error = Some("Usage shows after your next Codex turn.".to_string());
        return u;
    };
    u.plan = main.plan.as_deref().map(title);
    u.as_of = Some(main.at);
    let mut windows: Vec<_> = main.windows.iter().collect();
    windows.sort_by_key(|w| w.minutes);
    for w in windows {
        let (percent, resets_at) = current(w.used_percent, w.resets_at, now);
        let (key, label, window) = match w.minutes {
            300 => ("session".to_string(), "5h".to_string(), "5-hour window".to_string()),
            10080 => ("week".to_string(), "Week".to_string(), "Weekly".to_string()),
            m => (format!("window:{m}"), span(m), format!("{} window", span(m))),
        };
        u.meters.push(Meter { key, label, window, percent, resets_at, severity: severity(None, percent), model: None });
    }
    for r in latest.values().filter(|r| r.name.is_some() && now - r.at < 7 * DAY_MS) {
        let name = r.name.clone().unwrap_or_default();
        let fullest = r
            .windows
            .iter()
            .map(|w| (w, current(w.used_percent, w.resets_at, now)))
            .max_by(|a, b| a.1 .0.total_cmp(&b.1 .0));
        let Some((w, (percent, resets_at))) = fullest.filter(|(_, (p, _))| *p > 0.0) else { continue };
        u.meters.push(Meter {
            key: format!("model:{name}"),
            label: name.clone(),
            window: format!("{}, {name} only", if w.minutes == 10080 { "Weekly".to_string() } else { format!("{} window", span(w.minutes)) }),
            percent,
            resets_at,
            severity: severity(None, percent),
            model: Some(name),
        });
    }
    u
}

fn current(percent: f64, resets_at: Option<i64>, now: i64) -> (f64, Option<i64>) {
    match resets_at {
        Some(t) if t <= now => (0.0, None),
        other => (percent, other),
    }
}

fn span(minutes: i64) -> String {
    if minutes % 1440 == 0 {
        format!("{}d", minutes / 1440)
    } else if minutes % 60 == 0 {
        format!("{}h", minutes / 60)
    } else {
        format!("{minutes}m")
    }
}

fn title(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Where the account stands, for the Accounts dialog. Reads no secret: the keychain is asked
/// whether it holds a sign-in, and the email comes from the account's own settings file.
pub fn describe(a: &AccountCfg) -> AccountInfo {
    let dir = a.home_path();
    let (signed_in, email) = match a.harness.as_str() {
        "claude" => {
            // Claude Code keeps its settings in ~/.claude.json for the default folder, and
            // inside the folder otherwise.
            let settings = if dir == home().join(".claude") { home().join(".claude.json") } else { dir.join(".claude.json") };
            let email = std::fs::read(&settings)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .and_then(|v| v.pointer("/oauthAccount/emailAddress").and_then(Value::as_str).map(str::to_string));
            (claude_signed_in(&dir), email)
        }
        "opencode" => (crate::opencode::signed_in(&dir), None),
        // The sign-in lives beside the app data folder, under ~/.gemini.
        "antigravity" => (dir.parent().map(|p| p.join("jetski-standalone-oauth-token").is_file()).unwrap_or(false), None),
        _ => (dir.join("auth.json").is_file(), None),
    };
    AccountInfo {
        name: a.name.clone(),
        harness: a.harness.clone(),
        home: a.home.clone(),
        command: a.command.clone(),
        folder_exists: dir.is_dir(),
        signed_in,
        email,
    }
}

/// The command that signs an account in, typed into a new terminal window.
pub fn sign_in_command(a: &AccountCfg) -> String {
    match a.harness.as_str() {
        // Codex wants its folder to exist before it will use it.
        "codex" => format!("mkdir -p {} && {} login", a.home, codex_cli(a.command())),
        "opencode" => format!("{} auth login", a.command()),
        // Antigravity signs in from its own window.
        "antigravity" => "open -a Antigravity".to_string(),
        _ => a.command().to_string(),
    }
}

/// A Codex command as a terminal can run it. Installing the Codex app doesn't put `codex` on the
/// PATH, so a plain `codex` becomes the CLI inside the app when the PATH has none; a wrapper or
/// alias of your own is left as you wrote it.
fn codex_cli(command: &str) -> String {
    if with_program(command, "codex", "").is_none() || crate::detect::on_user_path("codex") {
        return command.to_string();
    }
    let Some(cli) = crate::detect::codex_app_cli() else { return command.to_string() };
    let quoted = format!("'{}'", cli.to_string_lossy().replace('\'', "'\\''"));
    with_program(command, "codex", &quoted).unwrap_or_else(|| command.to_string())
}

/// `command` with its program replaced, when the program (the first word past any `VAR=value`)
/// is `program`.
fn with_program(command: &str, program: &str, replacement: &str) -> Option<String> {
    let mut words: Vec<&str> = command.split_whitespace().collect();
    let assignment = |w: &str| w.split_once('=').is_some_and(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    let at = words.iter().position(|w| !assignment(w)).filter(|&i| words[i] == program)?;
    words[at] = replacement;
    Some(words.join(" "))
}

/// Reads every Claude account's usage, one after another. For the snapshot and the engine's
/// background fetch.
pub fn claude_all(accounts: &[AccountCfg]) -> Vec<Usage> {
    let now = now_ms();
    accounts.iter().filter(|a| a.harness == "claude").map(|a| claude(a, now)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex::RateWindow;

    #[test]
    fn swaps_only_the_program() {
        assert_eq!(with_program("codex", "codex", "'/A/codex'").as_deref(), Some("'/A/codex'"));
        assert_eq!(with_program("CODEX_HOME=~/.codex2 codex", "codex", "'/A/codex'").as_deref(), Some("CODEX_HOME=~/.codex2 '/A/codex'"));
        assert_eq!(with_program("codex2", "codex", "x"), None);
        assert_eq!(with_program("CODEX_HOME=codex other", "codex", "x"), None);
    }

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

    fn limit(name: Option<&str>, windows: &[(f64, i64, Option<i64>)], at: i64) -> RateLimit {
        RateLimit {
            name: name.map(str::to_string),
            plan: Some("pro".into()),
            windows: windows.iter().map(|&(used_percent, minutes, resets_at)| RateWindow { used_percent, minutes, resets_at }).collect(),
            at,
        }
    }

    #[test]
    fn codex_takes_the_newest_report_and_applies_resets() {
        let now = 1_000 * DAY_MS;
        let acct = AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: None };
        let a: BTreeMap<String, RateLimit> = [("codex".to_string(), limit(None, &[(40.0, 10080, Some(now + DAY_MS))], now - 5_000))].into();
        let b: BTreeMap<String, RateLimit> = [
            ("codex".to_string(), limit(None, &[(48.0, 10080, Some(now + DAY_MS))], now - 1_000)),
            ("codex_spark".to_string(), limit(Some("GPT-5.3-Codex-Spark"), &[(30.0, 300, Some(now - 10)), (12.0, 10080, Some(now + DAY_MS))], now - 1_000)),
        ]
        .into();
        let u = codex(&acct, [&a, &b].into_iter(), now);
        assert_eq!(u.plan.as_deref(), Some("Pro"));
        assert_eq!(u.meters.len(), 2);
        assert_eq!((u.meters[0].label.as_str(), u.meters[0].percent), ("Week", 48.0));
        // The Spark 5-hour window reset, so its week is the fullest.
        assert_eq!((u.meters[1].label.as_str(), u.meters[1].percent), ("GPT-5.3-Codex-Spark", 12.0));
    }

    #[test]
    fn codex_without_reports_says_so() {
        let acct = AccountCfg { name: "codex2".into(), harness: "codex".into(), home: "~/.codex2".into(), command: None };
        let u = codex(&acct, std::iter::empty(), 0);
        assert!(u.error.is_some() && u.meters.is_empty());
    }
}
