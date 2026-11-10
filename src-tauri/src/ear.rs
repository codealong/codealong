//! Hearing: codealong-ear (src-tauri/ear/main.swift), the helper process that captures audio and
//! turns speech into text on this Mac, and the way to it. One is started for each thing listened
//! to, a key held to talk (dictation.rs) or a listening (listen.rs), and is told what to listen
//! to: sources, each a stream of audio with a name, transcribed by itself, so who spoke is known by
//! where the sound came from. The microphone is you; what the Mac plays is everyone else.
//!
//! It's a child that speaks JSON lines (rpc.rs). `start` answers as soon as the sources are
//! capturing, the speech model made ready after it, so that nothing said meanwhile is missed;
//! what's heard then comes as `Event`s on the reader's thread, and `stop` answers once the last
//! words are out. No audio is kept anywhere: text is all that leaves the helper.
//!
//! Only macOS has it (Apple's SpeechAnalyzer, macOS 26), and only a build that was given the
//! helper: scripts/ear.sh builds it, and relaunch.sh, release.sh and engine.sh put it beside the
//! engine. Everything here that needs it fails in words for the person where it's missing.
//! What macOS asks the person before it lets Codealong hear is in .claude/rules/voice.md.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::os;
use crate::rpc::{self, Failure, Heard, Stderr, Wire};

/// The helper's name, beside the engine's binary.
const NAME: &str = "codealong-ear";

/// Names another helper to run, for a test or a build tried out of its bundle.
const EAR_VAR: &str = "CODEALONG_EAR";

/// How long the helper has to say its sources are capturing. Asking the person for the
/// microphone comes before it, and waits for their answer: `start_wait`.
const START: Duration = Duration::from_secs(20);

/// How long a start may wait when macOS has yet to ask the person for the microphone: their
/// answer is what it waits for.
const ASKING: Duration = Duration::from_secs(120);

/// How long the helper has to settle its last words once told to stop. It gives up on a model
/// that's stuck by itself (main.swift `finish`); this is for a helper that is.
const STOP: Duration = Duration::from_secs(25);

/// How long a question that starts nothing (`status`, `apps`) may take.
const ASK: Duration = Duration::from_secs(10);

/// Where a source's audio comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum From {
    /// The microphone. `cancel_echo` takes what the Mac plays out of it, for a meeting heard
    /// over speakers, where the others would be heard twice, once as you.
    Mic {
        #[serde(default)]
        cancel_echo: bool,
    },
    /// Everything the Mac plays.
    System,
    /// What these apps play, by bundle id.
    Apps { bundles: Vec<String> },
    /// A sound file, read at the speed it plays (`paced`) or as fast as it can be: what the
    /// checks listen to.
    File {
        path: String,
        #[serde(default)]
        paced: bool,
    },
}

/// One stream of audio to transcribe, and what its words are filed under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub name: String,
    #[serde(flatten)]
    pub from: From,
}

/// What to listen to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Spec {
    pub sources: Vec<Source>,
    /// The language spoken, as a locale ("en-US"). Empty for the Mac's own.
    pub locale: String,
    /// Words likely to be said that a speech model wouldn't guess: names of sessions, repos and
    /// projects.
    pub hints: Vec<String>,
}

/// Why the helper can't do something, with a word that tells the cases apart (`code`), since what
/// to offer the person depends on which it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refused {
    pub code: String,
    pub message: String,
}

/// `Refused::code` when the person hasn't let Codealong use the microphone.
pub const MICROPHONE_DENIED: &str = "microphone-denied";
/// `Refused::code` when this build or this Mac has nothing to hear with.
pub const UNAVAILABLE: &str = "unavailable";

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Refused {}

impl Refused {
    fn new(code: &str, message: impl Into<String>) -> Refused {
        Refused { code: code.into(), message: message.into() }
    }

    fn of(failure: Failure, doing: &str) -> Refused {
        match failure {
            Failure::Refused(e) => Refused {
                code: e.pointer("/data/code").and_then(Value::as_str).unwrap_or("failed").to_string(),
                message: e.get("message").and_then(Value::as_str).unwrap_or("It didn't say why.").to_string(),
            },
            Failure::Timeout => Refused::new("timeout", format!("Codealong's hearing took too long {doing}.")),
            Failure::Closed(last) => Refused::new("closed", stopped(&last)),
            Failure::Write(e) => Refused::new("closed", format!("Codealong's hearing stopped ({e}).")),
        }
    }
}

/// What's said of a helper that stopped by itself, with the last thing it printed.
pub fn stopped(last_words: &[String]) -> String {
    match last_words.last() {
        Some(last) => format!("Codealong's hearing stopped: {last}"),
        None => "Codealong's hearing stopped.".to_string(),
    }
}

/// Something the helper said.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The words a source is in the middle of, as the model has them so far: they change until
    /// they're settled, and are empty once they are.
    Partial { source: String, text: String },
    /// Words that are settled, with when they were said, in milliseconds since 1970.
    Final { source: String, text: String, at: i64, until: i64 },
    /// How loud a source is now, 0 to 1, about ten times a second.
    Level { source: String, level: f64 },
    /// The speech model is loaded, and what was said since the start is on its way.
    Ready,
    /// Something that holds things up for a while and passes: the model is being downloaded.
    Waiting { message: String },
    /// A source went wrong and nothing more comes from it; the others carry on.
    Lost { source: String, message: String },
    /// A source that ends by itself (a file) has, its last words settled.
    Ended { source: String },
    /// The speech model can't be had: nothing will be heard.
    Failed(Refused),
    /// The helper is gone. Always the last event.
    Closed { last_words: Vec<String> },
}

/// A line of the helper's as an event. None for one this build doesn't know.
pub fn event(line: &Value) -> Option<Event> {
    let params = line.get("params").unwrap_or(&Value::Null);
    let text = |key: &str| params.get(key).and_then(Value::as_str).unwrap_or("").to_string();
    let ms = |key: &str| (params.get(key).and_then(Value::as_f64).unwrap_or(0.0) * 1000.0).round() as i64;
    Some(match line.get("method").and_then(Value::as_str)? {
        "partial" => Event::Partial { source: text("source"), text: text("text") },
        "final" => Event::Final { source: text("source"), text: text("text"), at: ms("at"), until: ms("until") },
        "level" => Event::Level { source: text("source"), level: params.get("level").and_then(Value::as_f64).unwrap_or(0.0).clamp(0.0, 1.0) },
        "ready" => Event::Ready,
        "state" => Event::Waiting { message: text("message") },
        "lost" => Event::Lost { source: text("source"), message: text("message") },
        "ended" => Event::Ended { source: text("source") },
        "failed" => Event::Failed(Refused { code: text("code"), message: text("message") }),
        _ => return None,
    })
}

/// The helper to run, or why this build can't hear.
pub fn binary() -> Result<PathBuf, Refused> {
    if !os::is_macos() {
        return Err(Refused::new(UNAVAILABLE, os::only_on_macos("Hearing").to_string()));
    }
    if let Some(named) = std::env::var_os(EAR_VAR).map(PathBuf::from) {
        return Ok(named);
    }
    // Beside the engine, through whatever link the command was run by.
    let beside = std::env::current_exe().and_then(std::fs::canonicalize).ok().and_then(|exe| exe.parent().map(|dir| dir.join(NAME)));
    match beside {
        Some(path) if path.is_file() => Ok(path),
        _ => Err(Refused::new(UNAVAILABLE, "This build of Codealong was made without its hearing (scripts/ear.sh builds it).")),
    }
}

fn spawn(on: impl FnMut(Event) + Send + 'static) -> Result<rpc::Client, Refused> {
    let binary = binary()?;
    let mut on = on;
    rpc::Client::spawn(Command::new(&binary), Stderr::Tail, Wire::Bare, move |_, heard| match heard {
        Heard::Line(line) => {
            if let Some(event) = event(line) {
                on(event);
            }
        }
        Heard::Closed(last) => on(Event::Closed { last_words: last.to_vec() }),
    })
    .map_err(|e| Refused::new(UNAVAILABLE, format!("Codealong's hearing couldn't start ({e}). It needs macOS 26.")))
}

/// A helper that's listening. Dropped, it's killed; `stop` ends it with its last words heard.
pub struct Ear {
    client: rpc::Client,
}

impl Ear {
    /// Starts listening to `spec`, and returns once its sources are capturing. `on` gets what's
    /// heard, on a thread of the helper's own, `Event::Closed` last: it mustn't block for long,
    /// since what's said next waits behind it.
    pub fn start(spec: &Spec, on: impl FnMut(Event) + Send + 'static) -> Result<Ear, Refused> {
        let client = spawn(on)?;
        let mut params = json!({ "sources": spec.sources });
        if !spec.locale.trim().is_empty() {
            params["locale"] = json!(spec.locale.trim());
        }
        if !spec.hints.is_empty() {
            params["hints"] = json!(spec.hints);
        }
        // A microphone the person hasn't been asked about yet is asked for now, and the start
        // waits on their answer.
        let asking = spec.sources.iter().any(|s| matches!(s.from, From::Mic { .. }))
            && client.request("status", json!({}), Some(ASK)).is_ok_and(|s| s.get("microphone").and_then(Value::as_str) == Some("unasked"));
        client.request("start", params, Some(if asking { ASKING } else { START })).map_err(|e| Refused::of(e, "to start"))?;
        Ok(Ear { client })
    }

    /// Stops capturing and waits for the last words, which reach `on` before this returns; then
    /// the helper ends.
    pub fn stop(self) {
        let _ = self.client.request("stop", Value::Null, Some(STOP));
        self.client.end(Duration::from_secs(2));
    }
}

/// What this Mac can hear with, found without asking the person anything.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    /// Whether this Mac can transcribe speech at all.
    pub available: bool,
    /// Why not, when it can't.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// Whether Codealong may use the microphone: "allowed", "denied", "restricted", or
    /// "unasked" when macOS has yet to ask the person. Nothing says the same of the Mac's own
    /// audio: macOS gives silence until it's allowed, and no way to ask which it is.
    #[serde(default)]
    pub microphone: String,
    /// The locale speech is transcribed in, for the one asked about. None when macOS has no
    /// model for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    /// Whether that model is on this Mac already. One that isn't is downloaded at the first use.
    #[serde(default)]
    pub installed: bool,
}

fn ask(method: &str, params: Value, wait: Duration) -> Result<Value, Refused> {
    let client = spawn(|_| {})?;
    let answer = client.request(method, params, Some(wait)).map_err(|e| Refused::of(e, "to answer"));
    client.end(Duration::ZERO);
    answer
}

fn locale_params(locale: &str) -> Value {
    match locale.trim() {
        "" => json!({}),
        locale => json!({ "locale": locale }),
    }
}

/// What this Mac can hear with. A build or a Mac that can't hear says so here, not as an error.
pub fn status(locale: &str) -> Status {
    match ask("status", locale_params(locale), ASK) {
        Ok(answer) => {
            let mut status: Status = serde_json::from_value(answer).unwrap_or_default();
            if !status.available {
                status.why = Some("This Mac can't transcribe speech itself.".into());
            } else if status.locale.is_none() {
                status.why = Some(format!("macOS has no speech model for {}.", if locale.trim().is_empty() { "this Mac's language" } else { locale.trim() }));
            }
            status
        }
        Err(refused) => Status { why: Some(refused.message), ..Status::default() },
    }
}

/// An app that records or plays now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct App {
    pub bundle: String,
    pub name: String,
    #[serde(default)]
    pub recording: bool,
    #[serde(default)]
    pub playing: bool,
}

/// The apps that record or play now: what there is to pick from when only some should be heard,
/// and what a meeting is told from.
pub fn apps() -> Result<Vec<App>, Refused> {
    let answer = ask("apps", Value::Null, ASK)?;
    Ok(serde_json::from_value(answer.get("apps").cloned().unwrap_or(Value::Null)).unwrap_or_default())
}

/// Loads the speech model, downloading it first where it isn't on this Mac, so the next thing
/// said isn't kept waiting on it: the system keeps a loaded model a while.
pub fn warm(locale: &str) -> Result<(), Refused> {
    ask("warm", locale_params(locale), Duration::from_secs(600)).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_the_helper_says() {
        let line = |text: &str| serde_json::from_str::<Value>(text).unwrap();
        assert_eq!(event(&line(r#"{"method":"partial","params":{"source":"you","text":"hello or"}}"#)), Some(Event::Partial { source: "you".into(), text: "hello or".into() }));
        assert_eq!(
            event(&line(r#"{"method":"final","params":{"source":"them","text":"Hello.","at":1794283277.2016,"until":1794283278.4}}"#)),
            Some(Event::Final { source: "them".into(), text: "Hello.".into(), at: 1_794_283_277_202, until: 1_794_283_278_400 })
        );
        assert_eq!(event(&line(r#"{"method":"level","params":{"source":"you","level":1.7}}"#)), Some(Event::Level { source: "you".into(), level: 1.0 }));
        assert_eq!(event(&line(r#"{"method":"ready","params":{"locale":"en_US"}}"#)), Some(Event::Ready));
        assert_eq!(event(&line(r#"{"method":"state","params":{"state":"downloading","message":"Downloading"}}"#)), Some(Event::Waiting { message: "Downloading".into() }));
        assert_eq!(event(&line(r#"{"method":"lost","params":{"source":"you","message":"gone"}}"#)), Some(Event::Lost { source: "you".into(), message: "gone".into() }));
        assert_eq!(event(&line(r#"{"method":"ended","params":{"source":"file"}}"#)), Some(Event::Ended { source: "file".into() }));
        assert_eq!(event(&line(r#"{"method":"failed","params":{"code":"no-locale","message":"none"}}"#)), Some(Event::Failed(Refused { code: "no-locale".into(), message: "none".into() })));
        // A newer helper's line, and an answer nobody waits for any more.
        assert_eq!(event(&line(r#"{"method":"something-new","params":{}}"#)), None);
        assert_eq!(event(&line(r#"{"id":4,"result":{}}"#)), None);
    }

    #[test]
    fn sources_are_written_as_the_helper_reads_them() {
        let sources = vec![
            Source { name: "you".into(), from: From::Mic { cancel_echo: true } },
            Source { name: "them".into(), from: From::System },
            Source { name: "them".into(), from: From::Apps { bundles: vec!["us.example.meet".into()] } },
            Source { name: "file".into(), from: From::File { path: "/tmp/a.aiff".into(), paced: false } },
        ];
        assert_eq!(
            json!(sources),
            json!([
                { "name": "you", "kind": "mic", "cancel_echo": true },
                { "name": "them", "kind": "system" },
                { "name": "them", "kind": "apps", "bundles": ["us.example.meet"] },
                { "name": "file", "kind": "file", "path": "/tmp/a.aiff", "paced": false },
            ])
        );
        assert_eq!(serde_json::from_value::<Vec<Source>>(json!(sources)).unwrap(), sources);
    }

    #[test]
    fn a_refusal_keeps_the_helpers_word_for_it() {
        let refused = Refused::of(Failure::Refused(json!({ "code": -32000, "message": "Codealong isn't allowed to use the microphone.", "data": { "code": "microphone-denied" } })), "to start");
        assert_eq!(refused.code, MICROPHONE_DENIED);
        assert_eq!(refused.to_string(), "Codealong isn't allowed to use the microphone.");
        assert_eq!(Refused::of(Failure::Closed(vec!["dyld: symbol not found".into()]), "to start").message, "Codealong's hearing stopped: dyld: symbol not found");
        assert_eq!(Refused::of(Failure::Timeout, "to start").code, "timeout");
    }

    #[test]
    fn linux_has_no_hearing() {
        let refused = os::as_if(os::Os::Linux, binary).unwrap_err();
        assert_eq!(refused.code, UNAVAILABLE);
        assert_eq!(refused.message, "Hearing is only on macOS.");
    }
}
