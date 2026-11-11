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

use std::collections::HashSet;
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
    /// Whether macOS lets a source of the Mac's own audio be heard, as `Status::system_audio` has
    /// it: said as the source starts, and as it changes for as long as it isn't allowed.
    Permission { source: String, state: String },
    /// What the Mac plays through, by its id, with what the device says of itself (`kind`:
    /// "speakers", "headphones" or "unknown"): said as a source of the Mac's own audio starts,
    /// and when it changes (echo.rs).
    Output { device: String, kind: String },
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
        "permission" => Event::Permission { source: text("source"), state: text("state") },
        "output" => Event::Output { device: text("device"), kind: text("kind") },
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

    /// Has the microphone take what the Mac plays out of what it hears, or stop: you're on
    /// speakers, or no longer (echo.rs). Not waited on, so it can be said from the thread that
    /// reads what the helper says; one that didn't reach it is said again at its next start.
    pub fn echo(&self, on: bool) {
        let _ = self.client.notify("echo", json!({ "on": on }));
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
    /// "unasked" when macOS has yet to ask the person.
    #[serde(default)]
    pub microphone: String,
    /// Whether Codealong may hear what the Mac plays: "allowed", "denied", "unasked", or
    /// "unknown" (and empty from a helper that doesn't say) where macOS has no way to tell, when
    /// all there is to go by is that what isn't allowed is silent.
    #[serde(default, rename = "systemAudio")]
    pub system_audio: String,
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

/// An event in the person's calendar that has a time of day and wasn't called off, as the helper
/// reads it from macOS's own (EventKit): times in milliseconds since 1970.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Meeting {
    /// What its server names it with when it starts, which tells the occurrences of one that
    /// repeats apart, and is the same for its copy in each calendar that has it (meetings.rs
    /// `counted`).
    pub id: String,
    pub title: String,
    pub start: i64,
    pub end: i64,
    /// The calendar it's in, by name, and by the id that stays when it's renamed.
    pub calendar: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub calendar_id: String,
    /// The others in it, by name, or by address where the calendar has no name for them
    /// (`meetings`).
    pub attendees: Vec<String>,
    /// What the person answered in this calendar's copy of it: no, or yes (a maybe too).
    /// Neither while they haven't.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub declined: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub accepted: bool,
    /// The link to its call, found in its link, its place or its notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// For one that repeats, what its occurrences share.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
}

/// Someone in an event, as the calendar has them.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct Person {
    name: String,
    address: String,
    /// The calendar says they're the person: the owner of the account the event is in.
    me: bool,
    /// "yes" (a maybe too), "no", or nothing while they haven't answered.
    answer: String,
}

/// An event as the helper reads it: what a `Meeting` keeps of it, and the people in it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct Invited {
    #[serde(flatten)]
    meeting: Meeting,
    people: Vec<Person>,
}

/// The events the helper read as the person's meetings: who else is in each, and what the
/// person answered. The calendar says which of an event's people is them, but only of the
/// address of the account the event is in. Someone invited at two of their addresses has the
/// event in both accounts' calendars, and in each copy their other address is one more guest:
/// so an address the calendar calls theirs in any event read is theirs in all of them. What a
/// calendar or an account is called says nothing of whose an address is: a calendar someone
/// shares is named for theirs. The person's answer is the one beside the address this copy
/// calls theirs. Where it calls none of them theirs, it's what their addresses in it answered
/// between them, a yes at one over a no at another, as between a meeting's copies (meetings.rs
/// `counted`).
fn meetings(events: Vec<Invited>) -> Vec<Meeting> {
    let mine: HashSet<String> = events.iter().flat_map(|e| &e.people).filter(|p| p.me && !p.address.is_empty()).map(|p| p.address.to_lowercase()).collect();
    let is_mine = |p: &Person| p.me || mine.contains(&p.address.to_lowercase());
    events
        .into_iter()
        .map(|e| {
            let you: Vec<&Person> = match e.people.iter().find(|p| p.me) {
                Some(me) => vec![me],
                None => e.people.iter().filter(|p| is_mine(p)).collect(),
            };
            let said = |answer: &str| you.iter().any(|p| p.answer == answer);
            Meeting {
                attendees: e.people.iter().filter(|p| !is_mine(p)).filter_map(|p| [&p.name, &p.address].into_iter().find(|s| !s.is_empty()).cloned()).collect(),
                declined: said("no") && !said("yes"),
                accepted: said("yes"),
                ..e.meeting
            }
        })
        .collect()
}

/// An account macOS's calendar has calendars from: its name there (often its address) and its
/// kind ("icloud", "caldav" as Google's are, "exchange", "local", "subscribed", "birthdays").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CalendarSource {
    pub title: String,
    pub kind: String,
}

/// One of the person's calendars: its id, its name, and the account it's from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CalendarInfo {
    pub id: String,
    pub title: String,
    pub source: String,
}

/// The person's calendar as the helper read it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Calendar {
    /// Whether Codealong may read it: "allowed", "denied", "restricted", or "unasked" when macOS
    /// has yet to ask the person. Empty before the first read, and where the helper can't say.
    pub access: String,
    /// The accounts it has calendars from. Empty unless it's allowed.
    pub sources: Vec<CalendarSource>,
    /// Every calendar, by account then name.
    pub calendars: Vec<CalendarInfo>,
    /// The events between the times asked for, the first to start first.
    pub events: Vec<Meeting>,
}

/// Reads the person's calendar between `from` and `to` (milliseconds since 1970). With `ask`,
/// macOS asks them first whether Codealong may, when it hasn't yet, and this waits for their
/// answer; without, nobody is asked anything. Asked as whatever started the helper: Codealong
/// when its server does, a terminal when a shell does.
pub fn calendar(ask_first: bool, from: i64, to: i64) -> Result<Calendar, Refused> {
    let answer = ask("calendar", json!({ "ask": ask_first, "from": from, "to": to }), if ask_first { ASKING } else { ASK })?;
    read_calendar(answer).map_err(|e| Refused { code: "unread".into(), message: format!("The helper's answer about the calendar didn't read ({e}).") })
}

/// The helper's answer about the calendar, its events as the person's meetings (`meetings`).
fn read_calendar(mut answer: Value) -> serde_json::Result<Calendar> {
    let events = match answer.as_object_mut().and_then(|o| o.remove("events")) {
        Some(events) => serde_json::from_value(events)?,
        None => Vec::new(),
    };
    Ok(Calendar { events: meetings(events), ..serde_json::from_value(answer)? })
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
        assert_eq!(event(&line(r#"{"method":"permission","params":{"source":"them","state":"denied"}}"#)), Some(Event::Permission { source: "them".into(), state: "denied".into() }));
        assert_eq!(event(&line(r#"{"method":"output","params":{"source":"them","device":"BuiltInSpeakerDevice","kind":"speakers","name":"MacBook Pro Speakers"}}"#)), Some(Event::Output { device: "BuiltInSpeakerDevice".into(), kind: "speakers".into() }));
        assert_eq!(event(&line(r#"{"method":"ended","params":{"source":"file"}}"#)), Some(Event::Ended { source: "file".into() }));
        assert_eq!(event(&line(r#"{"method":"failed","params":{"code":"no-locale","message":"none"}}"#)), Some(Event::Failed(Refused { code: "no-locale".into(), message: "none".into() })));
        // What the helper says of itself, as it words it; an older one says nothing of the Mac's audio.
        let status: Status = serde_json::from_value(json!({ "available": true, "microphone": "allowed", "systemAudio": "unasked", "locale": "en_US", "installed": true })).unwrap();
        assert_eq!((status.microphone.as_str(), status.system_audio.as_str()), ("allowed", "unasked"));
        assert_eq!(serde_json::to_value(&status).unwrap()["systemAudio"], "unasked");
        assert_eq!(serde_json::from_value::<Status>(json!({ "available": true })).unwrap().system_audio, "");
        // The calendar as the helper answers it, and one it may not read.
        let read = read_calendar(json!({
            "access": "allowed",
            "sources": [{ "title": "sam@northwind.example", "kind": "caldav" }],
            "calendars": [{ "id": "C1", "title": "Work", "source": "sam@northwind.example" }],
            "events": [{ "id": "E1@1794330000000", "title": "Renewal call", "start": 1_794_330_000_000i64, "end": 1_794_331_800_000i64, "calendar": "Work", "calendarId": "C1", "people": [{ "name": "Sam Northwind", "address": "sam@northwind.example", "me": true, "answer": "yes" }, { "name": "Dana Kestrel", "address": "dana@kestrel.example" }], "link": "https://meet.example/abc", "series": "E1" }]
        }))
        .unwrap();
        assert_eq!((read.calendars[0].id.as_str(), read.events[0].calendar_id.as_str()), ("C1", "C1"));
        assert_eq!((read.access.as_str(), read.sources[0].kind.as_str(), read.events[0].link.as_deref()), ("allowed", "caldav", Some("https://meet.example/abc")));
        assert_eq!((read.events[0].attendees.clone(), read.events[0].accepted, read.events[0].declined), (vec!["Dana Kestrel".to_string()], true, false));
        assert_eq!(read_calendar(json!({ "access": "unasked" })).unwrap(), Calendar { access: "unasked".into(), ..Calendar::default() });
        assert!(read_calendar(json!({ "access": "allowed", "events": "none" })).is_err());
        // A newer helper's line, and an answer nobody waits for any more.
        assert_eq!(event(&line(r#"{"method":"something-new","params":{}}"#)), None);
        assert_eq!(event(&line(r#"{"id":4,"result":{}}"#)), None);
    }

    #[test]
    fn who_else_is_in_a_meeting_is_everyone_but_you_at_any_of_your_addresses() {
        let person = |name: &str, address: &str, me: bool, answer: &str| Person { name: name.into(), address: address.into(), me, answer: answer.into() };
        let event = |id: &str, calendar: &str, people: Vec<Person>| Invited { meeting: Meeting { id: id.into(), calendar: calendar.into(), ..Meeting::default() }, people };
        let dana = || person("Dana Kestrel", "dana@kestrel.example", false, "yes");
        let got = meetings(vec![
            // Invited at work and at home: the event is in both accounts' calendars, each
            // calling its own address yours and the other a guest. Neither calendar is named
            // for an address.
            event("m", "Work", vec![person("Sam Northwind", "sam@northwind.example", true, "yes"), person("", "sam@atlas.example", false, ""), dana()]),
            event("m", "Home", vec![person("Sam Northwind", "Sam@Northwind.example", false, "yes"), person("", "sam@atlas.example", true, ""), dana()]),
            // Only at work, with your home address a guest, which you said no at: your answer is
            // the one this calendar calls yours.
            event("n", "Work", vec![person("", "SAM@atlas.example", false, "no"), person("", "sam@northwind.example", true, "yes"), person("", "lee@atlas.example", false, "")]),
            // One that calls nobody you, where an address of yours said no.
            event("o", "Team", vec![person("", "sam@atlas.example", false, "no"), dana()]),
            // And ones where both of yours are, with different answers: a yes at one is a yes,
            // whichever comes first, and a no beside none yet is a no.
            event("q", "Team", vec![person("", "sam@atlas.example", false, "no"), person("", "sam@northwind.example", false, "yes"), dana()]),
            event("r", "Team", vec![person("", "sam@northwind.example", false, "yes"), person("", "sam@atlas.example", false, "no"), dana()]),
            event("s", "Team", vec![person("", "sam@northwind.example", false, ""), person("", "sam@atlas.example", false, "no"), dana()]),
            event("t", "Team", vec![person("", "sam@atlas.example", false, "no"), person("", "sam@northwind.example", false, ""), dana()]),
            // Someone's calendar they share with you, named for their address: they're no more
            // you for it, and nor is anyone in an event that has nobody of yours.
            event("p", "dana@kestrel.example", vec![dana(), person("Lee Atlas", "", false, "no"), person("", "", false, "")]),
        ]);
        let said: Vec<(&str, &str, Vec<&str>, bool, bool)> = got.iter().map(|m| (m.id.as_str(), m.calendar.as_str(), m.attendees.iter().map(String::as_str).collect(), m.accepted, m.declined)).collect();
        assert_eq!(
            said,
            vec![
                ("m", "Work", vec!["Dana Kestrel"], true, false),
                ("m", "Home", vec!["Dana Kestrel"], false, false),
                ("n", "Work", vec!["lee@atlas.example"], true, false),
                ("o", "Team", vec!["Dana Kestrel"], false, true),
                ("q", "Team", vec!["Dana Kestrel"], true, false),
                ("r", "Team", vec!["Dana Kestrel"], true, false),
                ("s", "Team", vec!["Dana Kestrel"], false, true),
                ("t", "Team", vec!["Dana Kestrel"], false, true),
                ("p", "dana@kestrel.example", vec!["Dana Kestrel", "Lee Atlas"], false, false),
            ]
        );
        // With no copy of it in the other account, nothing says that address is yours.
        let alone = meetings(vec![event("m", "Work", vec![person("", "sam@northwind.example", true, ""), person("", "sam@atlas.example", false, "")])]);
        assert_eq!(alone[0].attendees, vec!["sam@atlas.example".to_string()]);
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

    /// Against the real helper, on a Mac that can transcribe (macOS 26): `CODEALONG_EAR="$(
    /// scripts/ear.sh)" cargo test hears_a_recording -- --ignored --nocapture`. Has macOS say
    /// a few numbered sentences into a file and hears it back through the helper, as a meeting
    /// is heard: every sentence, in order, the first of them too, though the file is read before
    /// the speech model has loaded. Needs no microphone, and asks the person nothing.
    #[test]
    #[ignore]
    fn hears_a_recording() {
        if std::env::var_os(EAR_VAR).is_none() {
            return;
        }
        let dir = crate::fixtures::scratch("ear-recording");
        let file = dir.join("said.aiff");
        let said: Vec<String> = ["one", "two", "three", "four", "five"].iter().map(|n| format!("This is sentence number {n} of the recording.")).collect();
        let spoke = Command::new("/usr/bin/say").arg("-o").arg(&*file).arg(said.join(" [[slnc 500]] ")).status();
        assert!(spoke.is_ok_and(|s| s.success()), "macOS's say made no recording");
        let spec = Spec { sources: vec![Source { name: "Them".into(), from: From::File { path: file.to_string_lossy().into_owned(), paced: false } }], locale: "en-US".into(), hints: Vec::new() };
        let (tx, rx) = std::sync::mpsc::channel();
        let ear = Ear::start(&spec, move |event| {
            let _ = tx.send(event);
        })
        .expect("the helper starts");
        let mut heard = Vec::new();
        let mut partial = false;
        loop {
            match rx.recv_timeout(Duration::from_secs(120)).expect("the helper says something within two minutes") {
                Event::Final { source, text, at, until } => {
                    assert_eq!(source, "Them");
                    assert!(at > 0 && until >= at, "{at} to {until}");
                    heard.push(text);
                }
                Event::Partial { text, .. } => partial |= !text.is_empty(),
                Event::Ended { .. } => break,
                Event::Failed(refused) => panic!("{}", refused.message),
                Event::Closed { last_words } => panic!("{}", stopped(&last_words)),
                _ => {}
            }
        }
        ear.stop();
        let all = heard.join(" ").to_lowercase();
        // The model writes a number as a word or a figure, as it likes.
        let places: Vec<Option<usize>> = [["number one", "number 1"], ["number two", "number 2"], ["number three", "number 3"], ["number four", "number 4"], ["number five", "number 5"]].iter().map(|ways| ways.iter().find_map(|w| all.find(w))).collect();
        assert!(places.iter().all(Option::is_some), "not every sentence was heard: {heard:?}");
        assert!(places.windows(2).all(|w| w[0] < w[1]), "out of order: {heard:?}");
        assert!(partial, "no words came before they were settled");
    }

    #[test]
    fn linux_has_no_hearing() {
        let refused = os::as_if(os::Os::Linux, binary).unwrap_err();
        assert_eq!(refused.code, UNAVAILABLE);
        assert_eq!(refused.message, "Hearing is only on macOS.");
    }
}
