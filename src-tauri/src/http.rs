//! One HTTPS call to a provider's API as an account, through curl: Claude's plan usage
//! (claude/usage.rs) and the accounts' data settings (training.rs's readers). The headers, a
//! sign-in among them, and the body go to curl on stdin as its config, never on a command line,
//! and are never logged. Calls go over HTTP/1.1: Cloudflare, in front of chatgpt.com, answers
//! curl's HTTP/2 to its settings with a challenge whatever it sends, and lets HTTP/1.1 with a
//! User-Agent of Codealong's own through (2026-10-13).

use std::time::Duration;

use serde_json::Value;

use crate::procs;

/// What came back: the HTTP status and the body as text.
#[derive(Debug, PartialEq)]
pub struct Reply {
    pub status: u16,
    pub body: String,
}

impl Reply {
    /// The body as JSON; an empty one is null.
    pub fn json(&self) -> Option<Value> {
        if self.body.trim().is_empty() {
            return Some(Value::Null);
        }
        serde_json::from_str(&self.body).ok()
    }
}

/// A value for a double-quoted string in a curl config file, where backslash escapes apply.
fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The curl config for one call.
fn config(method: &str, url: &str, headers: &[String], body: Option<&Value>) -> String {
    let mut out = format!("url = {}\nrequest = {}\n", quoted(url), quoted(method));
    for h in headers.iter().map(String::as_str).chain(["Accept: application/json"]).chain(body.map(|_| "Content-Type: application/json")) {
        out.push_str(&format!("header = {}\n", quoted(h)));
    }
    // Codealong's own name, unless the caller gives one: curl would send both.
    if !headers.iter().any(|h| h.to_ascii_lowercase().starts_with("user-agent:")) {
        out.push_str(&format!("header = {}\n", quoted(&format!("User-Agent: codealong/{}", env!("CARGO_PKG_VERSION")))));
    }
    if let Some(body) = body {
        out.push_str(&format!("data-binary = {}\n", quoted(&body.to_string())));
    }
    out
}

/// Calls `url` with `headers` ("Name: value") and `body` as JSON, waiting up to `timeout`.
/// An error is in words: a sign-in that can't be sent, or why curl couldn't reach the server.
pub fn call(method: &str, url: &str, headers: &[String], body: Option<&Value>, timeout: Duration) -> Result<Reply, String> {
    // A line break in a header would start another one.
    if headers.iter().any(|h| h.chars().any(char::is_control)) {
        return Err("the stored sign-in is unreadable".into());
    }
    let secs = timeout.as_secs().max(1).to_string();
    let out = procs::run_stdin("/usr/bin/curl", &["-sS", "--http1.1", "-m", &secs, "-w", "\n%{http_code}", "-K", "-"], &config(method, url, headers, body), timeout + Duration::from_secs(5))
        .map_err(|e| e.to_string().trim().trim_start_matches("curl: ").chars().take(120).collect::<String>())?;
    let (body, status) = out.rsplit_once('\n').unwrap_or(("", &out));
    match status.trim().parse() {
        Ok(0) | Err(_) => Err("no answer".into()),
        Ok(status) => Ok(Reply { status, body: body.to_string() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sign_in_and_body_go_in_the_config_quoted() {
        let c = config("PATCH", "https://example.com/a", &["Authorization: Bearer t\"k".into()], Some(&serde_json::json!({"on": false, "w": "a\\b"})));
        assert_eq!(
            c,
            format!(
                "url = \"https://example.com/a\"\nrequest = \"PATCH\"\nheader = \"Authorization: Bearer t\\\"k\"\nheader = \"Accept: application/json\"\nheader = \"Content-Type: application/json\"\nheader = \"User-Agent: codealong/{}\"\ndata-binary = \"{{\\\"on\\\":false,\\\"w\\\":\\\"a\\\\\\\\b\\\"}}\"\n",
                env!("CARGO_PKG_VERSION")
            )
        );
    }

    #[test]
    fn a_callers_user_agent_replaces_codealongs() {
        let c = config("GET", "https://example.com", &["User-Agent: codealong/1 (antigravity)".into()], None);
        assert_eq!(c.matches("User-Agent").count(), 1);
        assert!(c.contains("codealong/1 (antigravity)"));
    }

    #[test]
    fn reads_the_status_after_the_body() {
        procs::stand_in::set(Some(Box::new(|program, args, input, _| {
            assert_eq!(program, "/usr/bin/curl");
            assert!(!args.iter().any(|a| a.contains("Bearer")), "the sign-in stays off the command line");
            assert!(input.unwrap().contains("Bearer abc"));
            Ok("{\"a\":1}\n200".to_string())
        })));
        let reply = call("GET", "https://example.com", &["Authorization: Bearer abc".into()], None, Duration::from_secs(5)).unwrap();
        assert_eq!(reply.status, 200);
        assert_eq!(reply.json(), Some(serde_json::json!({"a": 1})));
        assert_eq!(call("GET", "https://example.com", &["Authorization: Bearer a\nb".into()], None, Duration::from_secs(5)), Err("the stored sign-in is unreadable".into()));
        procs::stand_in::set(Some(Box::new(|_, _, _, _| Err(anyhow::anyhow!("curl: (6) Could not resolve host: example.com")))));
        assert_eq!(call("GET", "https://example.com", &[], None, Duration::from_secs(5)), Err("(6) Could not resolve host: example.com".into()));
        procs::stand_in::set(None);
    }
}
