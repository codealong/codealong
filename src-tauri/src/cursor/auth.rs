//! Cursor's sign-in and DashboardService API calls (plan usage, Privacy Mode).
//!
//! Cursor 3.30.14 (`cursorAuthTokenStorageCodec.js`) encrypts `cursorAuth/accessToken` in
//! `state.vscdb` at rest with Electron `safeStorage` (`cursor-safe-storage:v1:<base64>`) once
//! `cursor.storage.authTokenEncryptionAtRest` is on, so that value is ciphertext rather than a
//! bearer token. Meanwhile `cursor-agent` (2026.11.22, `../cli-credentials` and `../keychain`)
//! stores its sign-in in the macOS login keychain (`cursor-user` / `cursor-access-token`, written
//! through `/usr/bin/security -i` so reading it through `security` doesn't prompt), the Linux
//! Secret Service (`secret-tool`, service `cursor-credentials`, account `cursor-user`, JSON
//! `{accessToken}`), or `auth.json` (`AGENT_CLI_CREDENTIAL_STORE=file`).
//!
//! Tokens are never logged and never refreshed.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use crate::os::{self, Os};
use crate::procs;
use crate::util::file_name;

/// Cursor's API, where Privacy Mode and billing-period usage are read and set, as Cursor 3.22.7's
/// Settings and `cursor-agent` 2026.08.11+ do: Connect JSON over HTTP.
const DASHBOARD: &str = "https://api2.cursor.sh/aiserver.v1.DashboardService";
/// Prefix `cursorAuthTokenStorageCodec.js` puts on `cursorAuth/accessToken` in `state.vscdb` when
/// `cursor.storage.authTokenEncryptionAtRest` is on (Cursor 3.30.14+): ciphertext, not a token.
pub(super) const ENCRYPTED_TOKEN: &str = "cursor-safe-storage:";
const CLI_KEYCHAIN_ACCOUNT: &str = "cursor-user";
const CLI_KEYCHAIN_SERVICE: &str = "cursor-access-token";
const CLI_SECRET_SERVICE: &str = "cursor-credentials";
const KEYCHAIN_TIMEOUT: Duration = Duration::from_secs(10);

/// What a call says when Cursor turned its sign-in away.
pub(super) const EXPIRED: &str = "the sign-in has expired: sign in to Cursor again";

#[derive(Debug, PartialEq)]
pub(super) enum AppToken {
    Plain(String),
    Encrypted,
}

#[derive(Debug, PartialEq)]
pub(super) enum TokenErr {
    Missing { app_encrypted: bool },
    Api(String),
}

/// The app's `cursorAuth/accessToken` in `state.vscdb`: plaintext, or `cursor-safe-storage:v1:…`
/// ciphertext (Cursor 3.30.14+ with encryption at rest on).
pub(super) fn app_token(home: &Path) -> Option<AppToken> {
    let raw: String = super::app_db(home)
        .and_then(|p| crate::harness::open_database(&p))
        .and_then(|db| db.query_row("SELECT CAST(value AS TEXT) FROM ItemTable WHERE key = 'cursorAuth/accessToken'", [], |r| r.get(0)).ok())?;
    let raw = raw.trim();
    if raw.is_empty() {
        None
    } else if raw.starts_with(ENCRYPTED_TOKEN) {
        Some(AppToken::Encrypted)
    } else {
        Some(AppToken::Plain(raw.to_string()))
    }
}

/// Where `cursor-agent` keeps `auth.json`: in `home`, and on Linux/Windows for the default
/// `~/.cursor` account in its OS config folder (`$XDG_CONFIG_HOME/cursor`, `%APPDATA%\Cursor`).
pub(super) fn auth_file_paths(home: &Path) -> Vec<PathBuf> {
    let mut out = vec![home.join("auth.json")];
    if file_name(home) == ".cursor" && os::current() != Os::MacOs {
        if let Some(parent) = home.parent() {
            let os_file = os::dirs::app_data(parent, "Cursor", "cursor", "Roaming/Cursor").join("auth.json");
            if os_file != out[0] {
                out.push(os_file);
            }
        }
    }
    out
}

/// The command line's token from `auth.json`, when it keeps one there.
pub(super) fn file_token(home: &Path) -> Option<String> {
    auth_file_paths(home).into_iter().find_map(|p| {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()?;
        usable_token(v.get("accessToken").and_then(Value::as_str)?.to_string())
    })
}

/// Whether `auth.json` records a sign-in (tokens, an API key, or a pointer to the OS secret store,
/// not `{"signedOut": true}`).
pub(super) fn file_signed_in(home: &Path) -> bool {
    auth_file_paths(home).into_iter().any(|p| {
        let Some(v) = std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()) else { return false };
        if v.get("signedOut").and_then(Value::as_bool) == Some(true) {
            return false;
        }
        v.get("accessToken").and_then(Value::as_str).is_some_and(|t| usable_token(t.to_string()).is_some())
            || v.get("apiKey").and_then(Value::as_str).is_some_and(|k| !k.trim().is_empty())
            || v.get("storedIn").and_then(Value::as_str) == Some("os-secret-store")
    })
}

/// Whether the macOS login keychain holds a `cursor-agent` sign-in for the default `~/.cursor`
/// account, reading no secret.
pub(super) fn cli_store_has(home: &Path) -> bool {
    file_name(home) == ".cursor" && os::current() == Os::MacOs && os::secrets::keychain_has(CLI_KEYCHAIN_SERVICE, CLI_KEYCHAIN_ACCOUNT, KEYCHAIN_TIMEOUT)
}

/// The command line's token from the OS secret store: macOS login keychain (written through
/// `/usr/bin/security -i`) or Linux Secret Service (`secret-tool`).
pub(super) fn cli_store_token() -> Option<String> {
    let raw = match os::current() {
        Os::MacOs => os::secrets::keychain_read(CLI_KEYCHAIN_SERVICE, CLI_KEYCHAIN_ACCOUNT, KEYCHAIN_TIMEOUT)?,
        Os::Linux => {
            let out = procs::run("secret-tool", &["lookup", "service", CLI_SECRET_SERVICE, "account", CLI_KEYCHAIN_ACCOUNT], None, KEYCHAIN_TIMEOUT).ok()?;
            serde_json::from_str::<Value>(out.trim()).ok()?.get("accessToken").and_then(Value::as_str)?.to_string()
        }
        Os::Windows => return None,
    };
    usable_token(raw)
}

pub(super) fn usable_token(t: String) -> Option<String> {
    let t = t.trim();
    (!t.is_empty() && !t.starts_with(ENCRYPTED_TOKEN)).then(|| t.to_string())
}

pub(super) fn dashboard(token: &str, method: &str, body: &Value) -> Result<Value, String> {
    let headers = [format!("Authorization: Bearer {token}"), "Connect-Protocol-Version: 1".to_string()];
    let reply = crate::http::call("POST", &format!("{DASHBOARD}/{method}"), &headers, Some(body), Duration::from_secs(15))?;
    match reply.status {
        200..=299 => reply.json().ok_or_else(|| "unexpected response".to_string()),
        401 | 403 => Err(EXPIRED.to_string()),
        other => Err(format!("HTTP {other}")),
    }
}

/// Calls Cursor's API with the first sign-in it accepts: `auth.json` (this folder's own), then the
/// app's plaintext token (skipping `cursor-safe-storage:` ciphertext), then the command line's OS
/// secret store. A token Cursor turns down (`EXPIRED`) falls through to the next, so signing in
/// with `cursor-agent login` works even beside an expired app token.
pub(super) fn ask_tokens(
    file: Option<String>,
    app: Option<AppToken>,
    cli: impl FnOnce() -> Option<String>,
    mut ask: impl FnMut(&str) -> Result<Value, String>,
) -> Result<(Value, String), TokenErr> {
    let app_encrypted = app == Some(AppToken::Encrypted);
    let app_plain = match app {
        Some(AppToken::Plain(t)) => Some(t),
        _ => None,
    };
    let mut tried: Vec<String> = Vec::new();
    let mut last_err: Option<String> = None;
    for candidate in [file, app_plain].into_iter().flatten().chain(std::iter::once_with(cli).flatten()) {
        if tried.contains(&candidate) {
            continue;
        }
        match ask(&candidate) {
            Ok(v) => return Ok((v, candidate)),
            Err(why) if why == EXPIRED => {
                tried.push(candidate);
                last_err = Some(why);
            }
            Err(why) => return Err(TokenErr::Api(why)),
        }
    }
    match last_err {
        Some(why) => Err(TokenErr::Api(why)),
        None => Err(TokenErr::Missing { app_encrypted }),
    }
}

/// Whether `home` may fall back to the shared OS secret store (`cursor-user` /
/// `cursor-access-token`): `~/.cursor`, or a custom `CURSOR_CONFIG_DIR` folder that doesn't keep
/// its own `auth.json`.
fn allow_cli_store(home: &Path) -> bool {
    file_name(home) == ".cursor" || !home.join("auth.json").is_file()
}

pub(super) fn call_dashboard(home: &Path, method: &str, body: &Value) -> Result<(Value, String), TokenErr> {
    let allow_cli = allow_cli_store(home);
    ask_tokens(file_token(home), app_token(home), || allow_cli.then(cli_store_token).flatten(), |t| dashboard(t, method, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::Adapter as _;
    use crate::os::as_if;
    use rusqlite::Connection;
    use serde_json::json;

    /// Resets the thread-local process stand-in even if an assertion fails.
    struct StandInReset;
    impl Drop for StandInReset {
        fn drop(&mut self) {
            procs::stand_in::set(None);
        }
    }

    fn temp_home(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("codealong-cursor-auth-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_app_token(user_home: &Path, token: &str) {
        let db_path = super::super::app_db(&user_home.join(".cursor")).unwrap();
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        let db = Connection::open(&db_path).unwrap();
        db.execute("CREATE TABLE IF NOT EXISTS ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB)", []).unwrap();
        db.execute("INSERT INTO ItemTable (key, value) VALUES ('cursorAuth/accessToken', ?1)", [token]).unwrap();
    }

    /// Cursor 3.30.14 encrypts `cursorAuth/accessToken` in `state.vscdb` with a
    /// `cursor-safe-storage:v1:` prefix: it must never be sent as a bearer token, and the command
    /// line's keychain token is used instead.
    #[test]
    fn skips_encrypted_app_token_and_uses_cli_keychain() {
        let root = temp_home("encrypted");
        let home = root.join(".cursor");
        std::fs::create_dir_all(&home).unwrap();
        write_app_token(&root, "cursor-safe-storage:v1:dG9rZW4tY2lwaGVydGV4dA==");

        assert_eq!(app_token(&home), Some(AppToken::Encrypted));
        assert_eq!(usable_token("cursor-safe-storage:v1:abc".into()), None);

        let mut sent = Vec::new();
        let got = ask_tokens(None, app_token(&home), || Some("cli-jwt".into()), |tok| {
            sent.push(tok.to_string());
            Ok(json!({ "ok": true }))
        });
        assert_eq!(got, Ok((json!({ "ok": true }), "cli-jwt".into())));
        assert_eq!(sent, ["cli-jwt"], "ciphertext is never sent to Cursor's API");

        // Plaintext app tokens (older Cursor or encryption off) still work without touching the keychain.
        write_app_token(&root, "  plain-app-jwt  ");
        assert_eq!(app_token(&home), Some(AppToken::Plain("plain-app-jwt".into())));
        let mut cli_called = false;
        let got = ask_tokens(None, app_token(&home), || {
            cli_called = true;
            Some("cli-jwt".into())
        }, |tok| Ok(json!({ "tok": tok })));
        assert_eq!(got, Ok((json!({ "tok": "plain-app-jwt" }), "plain-app-jwt".into())));
        assert!(!cli_called, "keychain isn't read when a plaintext app token succeeds");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Signing in with `cursor-agent login` writes a fresh CLI token without replacing an expired
    /// plaintext app token in `state.vscdb`: a 401/403 falls through to the CLI token, while a
    /// non-auth error stops at once and duplicate tokens aren't sent twice.
    #[test]
    fn falls_back_to_cli_token_when_app_token_is_expired() {
        let mut sent = Vec::new();
        let got = ask_tokens(None, Some(AppToken::Plain("expired-app".into())), || Some("fresh-cli".into()), |tok| {
            sent.push(tok.to_string());
            if tok == "expired-app" { Err(EXPIRED.into()) } else { Ok(json!({ "ok": true })) }
        });
        assert_eq!(got, Ok((json!({ "ok": true }), "fresh-cli".into())));
        assert_eq!(sent, ["expired-app", "fresh-cli"]);

        // Same token in both places: only asked once.
        sent.clear();
        let dup = ask_tokens(None, Some(AppToken::Plain("same".into())), || Some("same".into()), |tok| {
            sent.push(tok.to_string());
            Err(EXPIRED.into())
        });
        assert_eq!(dup, Err(TokenErr::Api(EXPIRED.into())));
        assert_eq!(sent, ["same"]);

        // Non-auth error (offline, 503): stops without touching the CLI store.
        let mut cli_called = false;
        let outage = ask_tokens(None, Some(AppToken::Plain("app".into())), || {
            cli_called = true;
            Some("cli".into())
        }, |_| Err("HTTP 503".into()));
        assert_eq!(outage, Err(TokenErr::Api("HTTP 503".into())));
        assert!(!cli_called);

        // Encrypted app token with no CLI token reports app_encrypted so the UI can give the right guidance.
        assert_eq!(ask_tokens(None, Some(AppToken::Encrypted), || None, |_| unreachable!()), Err(TokenErr::Missing { app_encrypted: true }));
        assert_eq!(ask_tokens(None, None, || None, |_| unreachable!()), Err(TokenErr::Missing { app_encrypted: false }));
    }

    /// A command line sign-in kept in `auth.json` is read, `{"signedOut": true}` isn't treated as
    /// signed in, and a custom `CURSOR_CONFIG_DIR` folder with its own `auth.json` doesn't borrow
    /// `~/.cursor`'s OS secret store.
    #[test]
    fn reads_auth_json_and_respects_signed_out_and_custom_folders() {
        let dir = temp_home("file");
        std::fs::write(dir.join("auth.json"), r#"{"accessToken": " tok ", "refreshToken": "r", "apiKey": null}"#).unwrap();
        assert_eq!(file_token(&dir).as_deref(), Some("tok"));
        assert!(file_signed_in(&dir));
        assert!(!allow_cli_store(&dir), "custom folder with its own auth.json stays isolated");

        std::fs::write(dir.join("auth.json"), r#"{"signedOut": true}"#).unwrap();
        assert_eq!(file_token(&dir), None);
        assert!(!file_signed_in(&dir));

        std::fs::write(dir.join("auth.json"), r#"{"storedIn": "os-secret-store"}"#).unwrap();
        assert_eq!(file_token(&dir), None);
        assert!(file_signed_in(&dir));

        let _ = std::fs::remove_dir_all(&dir);

        // On Linux and Windows, `~/.cursor` also checks cursor-agent's OS config folder for `auth.json`.
        let root = temp_home("os-auth");
        let home = root.join(".cursor");
        std::fs::create_dir_all(&home).unwrap();
        let linux_file = root.join(".config/cursor/auth.json");
        std::fs::create_dir_all(linux_file.parent().unwrap()).unwrap();
        std::fs::write(&linux_file, r#"{"accessToken": "linux-tok"}"#).unwrap();
        assert_eq!(as_if(Os::Linux, || file_token(&home)).as_deref(), Some("linux-tok"));
        assert!(as_if(Os::Linux, || file_signed_in(&home)));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Reads the command line's OS secret store on macOS (keychain) and Linux (Secret Service),
    /// and `Cursor::signed_in` detects the macOS keychain item without reading the secret.
    #[test]
    fn reads_cli_secret_store_per_os_and_detects_keychain_sign_in() {
        let _reset = StandInReset;
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let log = seen.clone();
        procs::stand_in::set(Some(Box::new(move |program, args, _, _| {
            log.borrow_mut().push(format!("{program} {}", args.join(" ")));
            if program == "/usr/bin/security" {
                Ok(if args.contains(&"-w") { "  mac-jwt\n".into() } else { "keychain: login\n".into() })
            } else if program == "secret-tool" {
                Ok(r#"{"accessToken":"linux-jwt","refreshToken":"r"}"#.into())
            } else {
                Err(anyhow::anyhow!("unexpected {program}"))
            }
        })));

        assert_eq!(as_if(Os::MacOs, cli_store_token).as_deref(), Some("mac-jwt"));
        assert_eq!(as_if(Os::Linux, cli_store_token).as_deref(), Some("linux-jwt"));
        assert_eq!(as_if(Os::Windows, cli_store_token), None);

        let root = temp_home("kc-signed-in");
        let default_home = root.join(".cursor");
        let custom_home = root.join(".cursor-work");
        std::fs::create_dir_all(&default_home).unwrap();
        std::fs::create_dir_all(&custom_home).unwrap();
        assert_eq!(as_if(Os::MacOs, || super::super::Cursor.signed_in(&default_home)), (true, None));
        assert_eq!(as_if(Os::MacOs, || super::super::Cursor.signed_in(&custom_home)), (false, None));
        let _ = std::fs::remove_dir_all(&root);

        let calls = seen.borrow();
        assert_eq!(calls[0], "/usr/bin/security find-generic-password -a cursor-user -w -s cursor-access-token");
        assert_eq!(calls[1], "secret-tool lookup service cursor-credentials account cursor-user");
        assert_eq!(calls[2], "/usr/bin/security find-generic-password -a cursor-user -s cursor-access-token");
        assert_eq!(calls.len(), 3, "Windows and custom folders make no keychain call: {calls:?}");
    }

    /// `read_usage_with` clears the sign-in alert once a CLI token answers, asks to run
    /// `cursor-agent login` when the app's token is encrypted and the CLI is installed, and
    /// avoids a stuck sign-in alert when the CLI isn't installed.
    #[test]
    fn read_usage_handles_encrypted_app_token_with_and_without_cli() {
        let account = crate::config::AccountCfg {
            name: "cursor".into(),
            harness: "cursor".into(),
            home: temp_home("usage").to_string_lossy().into_owned(),
            command: None,
            start: None,
            acp: None,
            monthly: None,
            per_token: false,
            usage_cap: None,
            disabled: false,
            chrome_profile: None,
        };
        let usage_reply = json!({
            "billingCycleEnd": "1795997733015",
            "planUsage": { "autoPercentUsed": 2.0, "apiPercentUsed": 5.0, "totalPercentUsed": 3.0 }
        });
        let mut plan_tok = String::new();
        let ok = super::super::read_usage_with(
            &account,
            1_000,
            || Ok((usage_reply, "cli-jwt".into())),
            |tok| {
                plan_tok = tok.to_string();
                Ok(json!({ "planInfo": { "planName": "Pro+" } }))
            },
            || true,
        );
        assert!(!ok.needs_sign_in && !ok.no_limits && ok.error.is_none());
        assert_eq!(ok.plan.as_deref(), Some("Pro+"));
        assert_eq!(plan_tok, "cli-jwt");
        assert!(!ok.meters.is_empty());

        let with_cli = super::super::read_usage_with(&account, 1_000, || Err(TokenErr::Missing { app_encrypted: true }), |_| unreachable!(), || true);
        assert!(with_cli.needs_sign_in && !with_cli.no_limits);
        assert!(with_cli.error.as_deref().unwrap().contains("cursor-agent login"));

        let without_cli = super::super::read_usage_with(&account, 1_000, || Err(TokenErr::Missing { app_encrypted: true }), |_| unreachable!(), || false);
        assert!(!without_cli.needs_sign_in && without_cli.no_limits, "no stuck sign-in alert when only the encrypted app is installed");

        let signed_out = super::super::read_usage_with(&account, 1_000, || Err(TokenErr::Missing { app_encrypted: false }), |_| unreachable!(), || true);
        assert!(signed_out.needs_sign_in && signed_out.error.as_deref() == Some("Not signed in to Cursor"));

        let _ = std::fs::remove_dir_all(account.home_path());
    }
}
