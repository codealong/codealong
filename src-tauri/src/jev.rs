//! TypeSafe's Jev (docs.typesafe.ai): one endpoint that answers typed questions about a state
//! with a probability for every option. Codealong asks it narrow things, each feature behind its
//! own switch in `[jev]` (config.rs): where a prompt goes (route.rs), how a session's turn ended
//! (insights.rs), which todo a session is for (todos), which review comments ask for a change.
//!
//! The API key lives in the login keychain (service `Codealong TypeSafe`), never in config.toml
//! or on a command line: curl gets it and the request on stdin, and saving it goes through
//! `security -i`, which reads its command from stdin too. It's read from the keychain once and
//! kept in memory, since the engine asks often. `TYPESAFE_API_KEY` works when the keychain has
//! none, for the command line.
//!
//! Jev reads questions literally and is weak at dates and numbers: callers hand it times as
//! words (`ago_words`) and keep each question to one plain thing.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

const ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
const KEYCHAIN_SERVICE: &str = "Codealong TypeSafe";
const KEYCHAIN_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a key read from the keychain is used before it's read again, so a key replaced
/// from another place (the `security` command) is picked up.
const KEY_FOR: Duration = Duration::from_secs(10 * 60);

/// A request's body: the model, the state and the questions.
pub fn body(model: &str, state: Value, questions: Value) -> Value {
    json!({ "model": model, "state": state, "questions": questions })
}

/// Asks Jev. The answer is the whole response: `model`, `answers` by question id, `usage`.
pub fn ask(body: &Value) -> Result<Value> {
    let key = api_key().ok_or_else(|| anyhow!("There's no TypeSafe API key yet. Add it in Settings › Jev."))?;
    post(&key, body)
}

/// One call with a tiny question, for Settings' Test button: proves the key and says how fast.
pub fn check(model: &str) -> Result<String> {
    let started = Instant::now();
    let v = ask(&body(model, json!("Fix the flaky login test"), json!({ "task": { "type": "noul", "instructions": "Does this ask for a change to code?" } })))?;
    Ok(format!("{} answered in {} ms.", v["model"].as_str().unwrap_or(model), started.elapsed().as_millis()))
}

/* ---- Reading answers ---- */

/// A noul's answer, 0 (no) to 1 (yes).
pub fn noul(response: &Value, id: &str) -> Option<f64> {
    response["answers"][id]["noul"].as_f64()
}

/// A choice's options with their probabilities, the likeliest first.
pub fn choice(response: &Value, id: &str) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = response["answers"][id]["probabilities"]
        .as_object()
        .map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_f64()?))).collect())
        .unwrap_or_default();
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
    out
}

/// A score's value, from 0 to one less than its number of levels; it can land between levels.
pub fn score(response: &Value, id: &str) -> Option<f64> {
    response["answers"][id]["score"].as_f64()
}

/// How sure Jev was of an answer, 0 to 1, when the answer carries it.
pub fn confidence(response: &Value, id: &str) -> Option<f64> {
    response["answers"][id]["confidence"].as_f64()
}

/* ---- Words for the model ---- */

/// A time as words. Jev reads dates as text and can't compare them; words it can.
pub fn ago_words(then: i64, now: i64) -> String {
    let mins = (now - then).max(0) / 60_000;
    match mins {
        0..=1 => "just now".into(),
        2..=59 => format!("{mins} minutes ago"),
        60..=119 => "an hour ago".into(),
        120..=1439 => format!("{} hours ago", mins / 60),
        1440..=2879 => "yesterday".into(),
        _ => format!("{} days ago", mins / 1440),
    }
}

/// Text on one line and at most `max` characters, cut with an ellipsis.
pub fn clip(s: &str, max: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= max {
        return s;
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// The last `max` characters of a text, on one line: where a reply's question or its summary is.
pub fn tail(s: &str, max: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let n = s.chars().count();
    if n <= max {
        return s;
    }
    let mut out = String::from("…");
    out.extend(s.chars().skip(n - max + 1));
    out
}

/* ---- In the background ---- */

/// A request for the worker, and what its answer is for: the id comes back with it.
pub struct Job {
    pub id: String,
    pub body: Value,
}

/// A thread that asks Jev one request at a time and hands each answer to `done`. The engine
/// sends it jobs and hears back through its own channel, so a slow answer never holds up a
/// board.
pub fn worker(done: impl Fn(String, std::result::Result<Value, String>) + Send + 'static) -> Sender<Job> {
    let (tx, rx) = channel::<Job>();
    std::thread::Builder::new()
        .name("jev".into())
        .spawn(move || {
            for job in rx {
                let answer = ask(&job.body).map_err(|e| format!("{e:#}"));
                // Rate limited or overloaded: wait a moment before the next one.
                if answer.as_ref().err().is_some_and(|e| e.contains("rate limiting") || e.contains("overloaded")) {
                    std::thread::sleep(Duration::from_secs(5));
                }
                done(job.id, answer);
            }
        })
        .expect("spawning the jev thread");
    tx
}

/* ---- The request ---- */

/// Escapes a value for a double-quoted string in a curl config file.
fn curl_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The curl config for one request: the key and the body both go on curl's stdin.
fn curl_config(key: &str, body: &Value) -> String {
    format!(
        "url = {}\nrequest = \"POST\"\nheader = {}\nheader = \"Content-Type: application/json\"\nheader = \"Accept: application/json\"\nheader = \"User-Agent: codealong/{}\"\ndata-binary = {}\n",
        curl_quoted(ENDPOINT),
        curl_quoted(&format!("Authorization: Bearer {key}")),
        env!("CARGO_PKG_VERSION"),
        curl_quoted(&body.to_string()),
    )
}

fn post(key: &str, body: &Value) -> Result<Value> {
    if key.chars().any(|c| c.is_control() || c == '"' || c == '\\') {
        bail!("The TypeSafe API key has characters a key can't have. Paste it again in Settings › Jev.");
    }
    let mut child = Command::new("/usr/bin/curl")
        .args(["-sS", "-m", "15", "-w", "\n%{http_code}", "-K", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("curl: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(curl_config(key, body).as_bytes())?;
    }
    let out = child.wait_with_output().map_err(|e| anyhow!("curl: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let (reply, status) = text.rsplit_once('\n').unwrap_or(("", &text));
    let detail = || -> String {
        serde_json::from_str::<Value>(reply)
            .ok()
            .and_then(|v| v.get("detail").or_else(|| v.get("error")).or_else(|| v.get("message")).map(|d| d.to_string()))
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect()
    };
    match status.trim() {
        "200" => serde_json::from_str(reply).map_err(|_| anyhow!("TypeSafe sent an answer Codealong can't read.")),
        "401" | "403" => {
            forget_key();
            bail!("TypeSafe didn't accept the API key. Check it in Settings › Jev.")
        }
        "422" => bail!("TypeSafe couldn't take the request: {}", detail()),
        "429" => bail!("TypeSafe is rate limiting; try again in a moment."),
        "529" => bail!("TypeSafe is overloaded; try again in a moment."),
        "000" | "" => {
            let err = String::from_utf8_lossy(&out.stderr);
            bail!("Couldn't reach TypeSafe: {}", err.trim().trim_start_matches("curl: ").chars().take(160).collect::<String>())
        }
        other => bail!("TypeSafe answered HTTP {other}. {}", detail()),
    }
}

/* ---- The key ---- */

static KEY: Mutex<Option<(Instant, String)>> = Mutex::new(None);

pub(crate) fn keychain_account() -> String {
    std::env::var("USER").ok().filter(|u| !u.is_empty() && u.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))).unwrap_or_else(|| "codealong".into())
}

fn forget_key() {
    if let Ok(mut k) = KEY.lock() {
        *k = None;
    }
}

/// The API key: the keychain's, else `TYPESAFE_API_KEY`.
fn api_key() -> Option<String> {
    if let Some((at, key)) = KEY.lock().ok().and_then(|k| k.clone()) {
        if at.elapsed() < KEY_FOR {
            return Some(key);
        }
    }
    let key = crate::procs::run("/usr/bin/security", &["find-generic-password", "-a", &keychain_account(), "-s", KEYCHAIN_SERVICE, "-w"], None, KEYCHAIN_TIMEOUT)
        .ok()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .or_else(|| std::env::var("TYPESAFE_API_KEY").ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty()))?;
    if let Ok(mut k) = KEY.lock() {
        *k = Some((Instant::now(), key.clone()));
    }
    Some(key)
}

/// Whether there's a key, without reading it.
pub fn has_key() -> bool {
    crate::procs::run("/usr/bin/security", &["find-generic-password", "-a", &keychain_account(), "-s", KEYCHAIN_SERVICE], None, KEYCHAIN_TIMEOUT).is_ok()
        || std::env::var("TYPESAFE_API_KEY").is_ok_and(|k| !k.trim().is_empty())
}

/// Saves the key in the keychain, replacing one that's there. The command goes to `security`
/// on stdin, so the key never shows in the process list.
pub fn save_key(key: &str) -> Result<()> {
    let key = key.trim();
    if key.is_empty() || key.chars().any(|c| c.is_control() || c.is_whitespace() || c == '"' || c == '\\') {
        bail!("That doesn't look like an API key.");
    }
    let command = format!("add-generic-password -U -a \"{}\" -s \"{}\" -l \"{}\" -w \"{}\"\n", keychain_account(), KEYCHAIN_SERVICE, KEYCHAIN_SERVICE, key);
    let out = crate::procs::run_stdin("/usr/bin/security", &["-i"], &command, KEYCHAIN_TIMEOUT)?;
    forget_key();
    // `security -i` exits cleanly whatever its command did; errors come back as text.
    if out.to_lowercase().contains("error") || !has_key() {
        bail!("The keychain didn't take the key.");
    }
    Ok(())
}

pub fn delete_key() -> Result<()> {
    let _ = crate::procs::run("/usr/bin/security", &["delete-generic-password", "-a", &keychain_account(), "-s", KEYCHAIN_SERVICE], None, KEYCHAIN_TIMEOUT);
    forget_key();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 10 * 24 * 3600 * 1000;

    #[test]
    fn times_go_in_as_words() {
        assert_eq!(ago_words(NOW - 30_000, NOW), "just now");
        assert_eq!(ago_words(NOW - 5 * 60_000, NOW), "5 minutes ago");
        assert_eq!(ago_words(NOW - 3 * 3_600_000, NOW), "3 hours ago");
        assert_eq!(ago_words(NOW - 30 * 3_600_000, NOW), "yesterday");
        assert_eq!(ago_words(NOW - 4 * 86_400_000, NOW), "4 days ago");
    }

    #[test]
    fn clips_the_start_and_keeps_the_end() {
        assert_eq!(clip("one  two\nthree", 20), "one two three");
        assert_eq!(clip("abcdefghij", 5), "abcd…");
        assert_eq!(tail("abcdefghij", 5), "…ghij");
        assert_eq!(tail("short", 10), "short");
    }

    #[test]
    fn reads_answers() {
        let v = json!({ "answers": {
            "a": { "type": "noul", "noul": 0.9 },
            "b": { "type": "choice", "choice": "x", "probabilities": { "x": 0.7, "y": 0.3 } },
            "c": { "type": "score", "score": 1.4, "confidence": 0.55 },
        }});
        assert_eq!(noul(&v, "a"), Some(0.9));
        assert_eq!((confidence(&v, "c"), confidence(&v, "a")), (Some(0.55), None));
        assert_eq!(choice(&v, "b"), vec![("x".to_string(), 0.7), ("y".to_string(), 0.3)]);
        assert_eq!(score(&v, "c"), Some(1.4));
        assert_eq!(noul(&v, "missing"), None);
    }

    #[test]
    fn the_key_and_body_are_escaped_for_curl() {
        let body = json!({ "state": "say \"hi\"\nback\\slash" });
        let cfg = curl_config("tsk_123", &body);
        assert!(cfg.contains("header = \"Authorization: Bearer tsk_123\"\n"));
        assert!(cfg.contains(r#"data-binary = "{\"state\":\"say \\\"hi\\\"\\nback\\\\slash\"}""#), "{cfg}");
        assert!(cfg.starts_with("url = \"https://api.typesafe.ai/v1/systemone\"\n"));
    }
}
