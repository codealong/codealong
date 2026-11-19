//! `codealong listen`: listenings, each a session that hears a meeting on this Mac (listen.rs,
//! meetings.rs), while `[experimental] voice` is on. The server does the hearing, so `start` and
//! `stop` need it. `list` is read from the listenings' own file and needs none. `status` asks the
//! server, and with none it's this shell that answers, which it says: what macOS lets a terminal
//! hear with isn't what it lets Codealong. `ListenRow` says where a listening's transcript and
//! notes are: an agent asked about a meeting has only that to find them by.

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;

use crate::actions::{self, params};
use crate::util::{self, now_ms};

use super::*;

/// What `codealong listen status` says of your calendar: nothing while meetings aren't offered.
fn calendar_said(c: &crate::meetings::Status) -> Option<String> {
    if !c.on {
        return None;
    }
    if let Some(problem) = &c.problem {
        return Some(format!("Your calendar couldn't be read: {problem}"));
    }
    Some(match c.access.as_str() {
        "allowed" if c.sources.is_empty() => "Codealong may read your calendar, which has no accounts in it: add yours in System Settings › Internet Accounts, with Calendars on.".to_string(),
        "allowed" => {
            let from: Vec<String> = c.sources.iter().map(|s| format!("{} ({})", s.title, s.kind)).collect();
            let ahead = match (c.meetings, c.next_at) {
                (0, _) => "No meetings in the day ahead.".to_string(),
                (n, Some(at)) => format!("{n} {} in the day ahead, the next at {}.", if n == 1 { "meeting" } else { "meetings" }, crate::meetings::hour(at)),
                (n, None) => format!("{n} {} in the day ahead.", if n == 1 { "meeting" } else { "meetings" }),
            };
            format!("Codealong reads the calendars of {}. {ahead}", util::listed(&from, "and"))
        }
        "unasked" => "macOS hasn't asked yet whether Codealong may read your calendar: Settings › Experimental asks.".to_string(),
        "" => "Your calendar hasn't been read yet.".to_string(),
        _ => "Codealong isn't allowed to read your calendar: System Settings › Privacy & Security › Calendars, with full access.".to_string(),
    })
}

/// A listening as `listen list` gives it: its record, with what its session is called and where
/// its transcript and notes are, which the record leaves to be worked out from its id or its
/// meeting's folder. An agent asked about a meeting has only this to find it by.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListenRow<'a> {
    #[serde(flatten)]
    listening: &'a crate::listen::Listening,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    transcript: std::path::PathBuf,
    /// Only when there are some: a coach keeps none, and a note-taker's can have been removed.
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<std::path::PathBuf>,
}

impl<'a> ListenRow<'a> {
    fn of(listening: &'a crate::listen::Listening, title: Option<String>) -> Self {
        ListenRow {
            listening,
            title: title.map(|title| title.trim().to_string()).filter(|title| !title.is_empty()),
            transcript: listening.transcript(),
            notes: Some(listening.notes()).filter(|notes| notes.is_file()),
        }
    }
}

/// `codealong listen`. The server keeps the listenings and runs their hearing, so everything but
/// the list needs it.
pub(super) fn listen(cmd: ListenCmd, c: &mut Client, json: bool) -> Result<()> {
    use crate::listen::{self, Ask, Listening, Role};
    let cfg = c.cfg()?.clone();
    listen::turned_on(&cfg)?;
    let needs_server = |c: &Client| match c.app {
        Some(_) => Ok(()),
        None => Err(anyhow!("A listening needs Codealong's server, which couldn't be started here (`codealong server start` says why): it does the hearing.")),
    };
    match cmd {
        ListenCmd::List => {
            let all = listen::list();
            // What each one's session is called, where the board still has it. The list is the
            // file's, and is still one when no board can be had.
            let titles: Vec<Option<String>> = match c.board() {
                Ok(board) => all.iter().map(|l| board.session(&l.session).map(|(_, s)| s.title.clone())).collect(),
                Err(_) => vec![None; all.len()],
            };
            let listed: Vec<ListenRow> = all.iter().zip(titles).map(|(l, title)| ListenRow::of(l, title)).collect();
            if json {
                return print_json(&listed);
            }
            let now = now_ms();
            let rows = listed
                .iter()
                .map(|listed| {
                    let l = listed.listening;
                    // One with no session to name it yet, or any more, goes by its meeting.
                    let meeting = || l.meeting.as_ref().map(|m| m.title.trim().to_string()).filter(|title| !title.is_empty());
                    let title = listed.title.clone().or_else(meeting).unwrap_or_else(|| l.session.clone());
                    let state = match (l.ended_at, &l.why_ended) {
                        (None, _) => format!("hearing, {}", ago(now - l.started_at).replace(" ago", "")),
                        (Some(at), Some(why)) => format!("ended {} ({why})", ago(now - at)),
                        (Some(at), None) => format!("ended {}", ago(now - at)),
                    };
                    vec![title, state, format!("{} of {} told", l.told, l.lines), crate::util::tilde(&listed.transcript)]
                })
                .collect();
            print!("{}", table_text(&["SESSION", "STATE", "LINES", "TRANSCRIPT"], rows));
            Ok(())
        }
        ListenCmd::Status => {
            // Asked of the server, which does the hearing: macOS answers for whoever asks whether
            // the microphone may be used, and this shell isn't Codealong. With no server it's this
            // shell that's asked, which is said.
            let served = c.app.is_some();
            let me = find_me(c)?.0;
            let mut heard = c.act(actions::HEARING, "", serde_json::json!({}), me)?;
            let status: crate::ear::Status = serde_json::from_value(heard["status"].clone())?;
            let apps: Vec<crate::ear::App> = serde_json::from_value(heard["apps"].clone()).unwrap_or_default();
            // Where the meetings Codealong took are, a folder each: the one list of them there
            // is, which an agent asked about an old one has to be told the place of.
            let meetings_folder = crate::meetings::root(&cfg);
            if json {
                // Whose answer it is, for whoever reads only this: `shell` is this command's own
                // terminal, whose microphone isn't Codealong's.
                heard["answeredBy"] = serde_json::json!(if served { "server" } else { "shell" });
                heard["meetingsFolder"] = serde_json::json!(meetings_folder);
                return print_json(&heard);
            }
            if !served {
                println!("Codealong's server isn't running, so this is what this shell's terminal can hear with; what it says of the microphone and of what this Mac plays is the terminal's answer, not Codealong's.");
            }
            match &status.why {
                Some(why) => println!("{why}"),
                None => {
                    let locale = status.locale.as_deref().unwrap_or_default();
                    println!("Speech is transcribed on this Mac, in {locale}{}.", if status.installed { "" } else { " once its model has been downloaded, at the first use" });
                    println!(
                        "{}",
                        match status.microphone.as_str() {
                            "allowed" => "Codealong may use the microphone.",
                            "unasked" => "macOS hasn't asked yet whether Codealong may use the microphone: it will at the first use.",
                            _ => "Codealong isn't allowed to use the microphone: System Settings › Privacy & Security › Microphone.",
                        }
                    );
                    // Nothing is said where macOS couldn't be asked (or the helper is an older one).
                    match status.system_audio.as_str() {
                        "allowed" => println!("Codealong may hear what this Mac plays."),
                        "unasked" => println!("macOS hasn't asked yet whether Codealong may hear what this Mac plays: it will the first time a session listens to it."),
                        "denied" => println!("Codealong isn't allowed to hear what this Mac plays: System Settings › Privacy & Security › Screen & System Audio Recording, under System Audio Recording Only."),
                        _ => {}
                    }
                    // The calendar, where meetings are offered (meetings.rs), as the server last read it.
                    if let Ok(calendar) = serde_json::from_value::<crate::meetings::Status>(heard["calendar"].clone()) {
                        if let Some(said) = calendar_said(&calendar) {
                            println!("{said}");
                        }
                    }
                    for app in &apps {
                        let does = match (app.recording, app.playing) {
                            (true, true) => "records and plays",
                            (true, false) => "records",
                            _ => "plays",
                        };
                        println!("{} ({}) {does}.", app.name, app.bundle);
                    }
                }
            }
            println!("The meetings Codealong took are kept in {}, a folder each.", crate::util::tilde(&meetings_folder));
            Ok(())
        }
        ListenCmd::Start { session, file, at_speed, role, start_sessions, prompt } => {
            needs_server(c)?;
            let key = resolve_session(c, &session)?;
            let path = dunce::canonicalize(crate::util::expand(&file)).with_context(|| format!("{file} isn't there to hear."))?;
            let role = match role {
                RoleArg::Notes => Role::Notes,
                RoleArg::Coach => Role::Coach,
                RoleArg::Requests => Role::Requests,
                RoleArg::Custom => Role::Custom,
            };
            if start_sessions && role != Role::Requests {
                bail!("--start-sessions goes with --as requests.");
            }
            let ask = Ask { file: Some(path.to_string_lossy().into_owned()), paced: at_speed, role, start: start_sessions, prompt: prompt.unwrap_or_default(), ..Ask::default() };
            let me = find_me(c)?.0;
            let l: Listening = serde_json::from_value(c.act(actions::LISTEN, &key, params::Listen { on: true, ask }, me)?)?;
            if json {
                return print_json(&l);
            }
            println!("It's hearing {}. The transcript: {}", crate::util::tilde(&path), crate::util::tilde(&l.transcript()));
            Ok(())
        }
        ListenCmd::Stop { session } => {
            needs_server(c)?;
            let key = resolve_session(c, &session)?;
            let me = find_me(c)?.0;
            let l: Listening = serde_json::from_value(c.act(actions::LISTEN, &key, params::Listen { on: false, ask: Ask::default() }, me)?)?;
            if json {
                return print_json(&l);
            }
            println!("Ended. Its session is told the last of it. The transcript: {}", crate::util::tilde(&l.transcript()));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_listed_listening_says_where_its_transcript_and_notes_are() {
        let dir = crate::store::test_dir("listen-list");
        let l = crate::listen::Listening { id: "abc".into(), session: "claude:main:1".into(), lines: 12, folder: Some(dir.display().to_string()), ..Default::default() };
        // A coach's, or one whose notes aren't written yet: only the transcript.
        let said = serde_json::to_value(ListenRow::of(&l, Some("  Renewal call ".into()))).unwrap();
        assert_eq!(said["transcript"], dir.join("transcript.md").display().to_string());
        assert_eq!(said["title"], "Renewal call");
        assert!(said.get("notes").is_none(), "{said}");
        // Its record's own fields are beside them, as they were.
        assert_eq!((&said["id"], &said["session"], &said["lines"]), (&json!("abc"), &json!("claude:main:1"), &json!(12)));
        std::fs::write(dir.join("notes.md"), "# Renewal call\n").unwrap();
        // A session the board no longer has, or one with no name yet, gives it no title.
        for title in [None, Some(" ".to_string())] {
            let said = serde_json::to_value(ListenRow::of(&l, title)).unwrap();
            assert_eq!(said["notes"], dir.join("notes.md").display().to_string());
            assert!(said.get("title").is_none(), "{said}");
        }
        // One that isn't a meeting's is in a folder of its own among Codealong's files.
        let own = crate::listen::Listening { id: "abc".into(), ..Default::default() };
        assert!(ListenRow::of(&own, None).transcript.ends_with("listenings/abc/transcript.md"));
    }
}
