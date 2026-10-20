//! Messages between Claude Code sessions: the envelope one goes in, and the socket a live
//! session listens on for them (`messagingSocketPath`), which is a way in (`Message`). A
//! Claude app session has no other: nothing there takes typing from outside.
//!
//! A session that skips permission prompts holds such a message for you to allow unless it
//! claims the session's own permission class, and the Claude app has no way to show a held one.
//! What you type in Codealong's windows on this Mac, or on your phone, which runs no agents, is
//! yours, as if typed in the session, so it makes that claim; from another Mac it doesn't, since
//! an agent there may have asked for it, and from an agent only when the agent's own session skips
//! permission prompts too, as Claude Code's own messages between sessions claim their sender's
//! class, or when the agent is a helper the session it writes to started, by a start that said
//! which session it made, or the automatic review of a PR that session opened (send.rs
//! `from_its_helper`), whose message is work that session asked for and would read anyway. So an
//! agent can't use Codealong to get past the hold of a session it doesn't work for, and a Claude
//! app session is told nothing rather than holding a message no one will see.

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use serde_json::json;

use crate::attachments::{self, Style};
use crate::model::Surface;
use crate::send::{Delivery, Own, Sent, Way};

/// The sender's name on what you send, and the line its text starts with. The session's
/// transcript keeps both (conversation.rs, `sent_from_codealong`), which is how its panel knows the
/// message was yours. The line says the answer goes in the session's reply: no session sent it,
/// so the envelope names nothing to answer, and an agent that answered with SendMessage, as
/// Claude Code tells it to answer another session, sent it to whichever session's name begins
/// with "Codealong" (2026-10-14).
pub const FROM_CODEALONG: &str = "Codealong";
pub const FROM_YOU: &str = "Your user sent this to this session from Codealong, where they read your replies. No session sent it, so answer here, not with SendMessage:";

/// The prompt as a cross-session message. The envelope is the one Claude Code writes for its
/// own sends (`<cross-session-message from-name="…">`, the body on lines of its own), so the
/// session shows who it's from: Codealong for what you sent, or `from`, an agent's session, whose
/// text carries its own byline. `claim` is the permission class it claims (`bypass` or
/// `prompting`, `from-mode`), which Claude Code compares with the session's own: a session
/// that skips prompts holds a message that claims none, or another, for you to allow. The
/// attributes go in the order Claude Code writes them, since it reads the envelope only when
/// writing it again gives the same text.
pub fn peer_content(text: &str, claim: Option<&str>, from: Option<&str>) -> String {
    let name = from.map(envelope_name).unwrap_or_else(|| FROM_CODEALONG.to_string());
    let body = peer_body(text, from).replace("</cross-session-message", "<\\/cross-session-message");
    let mode = claim.map(|m| format!(" from-mode=\"{m}\"")).unwrap_or_default();
    format!("<cross-session-message from-name=\"{name}\"{mode}>\n{body}\n</cross-session-message>")
}

/// The text inside the envelope: an agent's as it is (it carries its own byline), yours after
/// the line that says it's yours.
fn peer_body(text: &str, from: Option<&str>) -> String {
    match from {
        Some(_) => text.trim().to_string(),
        None => format!("{FROM_YOU}\n\n{}", text.trim()),
    }
}

/// A sender's name as Claude Code writes `from-name` (2.1.284): without quotes, angle brackets,
/// or control and format characters, trimmed, and at most 64 characters, a longer one cut with
/// "…". Anything else would come out different when Claude Code writes the envelope again, and
/// it would then not read it as one. This keeps letters, digits, spaces and punctuation, which
/// come out the same.
fn envelope_name(name: &str) -> String {
    let kept: String = name
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|&c| c.is_alphanumeric() || c == ' ' || (c.is_ascii_punctuation() && !matches!(c, '"' | '<' | '>')) || "“”‘’–—…·".contains(c))
        .collect();
    let kept = kept.split(' ').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = kept.chars().collect();
    match chars.len() {
        0 => "codealong".to_string(),
        n if n > 64 => chars[..63].iter().collect::<String>().trim_end().to_string() + "…",
        _ => kept,
    }
}

/// The line written to a session's socket. `session_id` makes a session that has since taken
/// over the pid drop it.
pub fn peer_frame(session_id: &str, msg_id: &str, text: &str, claim: Option<&str>, from: Option<&str>) -> String {
    let frame = json!({
        "msgV": 1,
        "msg_id": msg_id,
        "type": "user",
        "session_id": session_id,
        "message": { "role": "user", "content": peer_content(text, claim, from) },
        "priority": "next",
    });
    format!("{frame}\n")
}

/// Nothing comes back on the connection, so a successful write is all there is to see. Claude
/// Code's own sender waits a moment before closing, so the line is read before the socket goes.
fn peer_send(socket: &str, session_id: &str, text: &str, claim: Option<&str>, from: Option<&str>) -> Result<()> {
    let frame = peer_frame(session_id, &crate::start::new_session_id(), text, claim, from);
    // Claude Code caps a line at a million characters.
    if frame.len() > 1_000_000 {
        bail!("That's too long to send to another session; shorten it or put it in a file.");
    }
    let mut stream = UnixStream::connect(socket).map_err(|e| anyhow!("The session's message socket didn't answer: {e}"))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(frame.as_bytes())?;
    stream.flush()?;
    std::thread::sleep(Duration::from_millis(150));
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Ok(())
}

/// What a prompt sent on a session's socket says it did. `untyped` is why it wasn't typed into
/// the session's terminal, when that was tried first: a session reads a message from another
/// session as a teammate's, not as yours, so it says it went that way, first, since the bar
/// shows the start of it. `held`: the session skips permission prompts and the message claims
/// no mode, so it asks you (or, for an agent's, its user) first.
fn peer_note(title: &str, untyped: Option<&str>, busy: bool, held: bool, agent: bool) -> String {
    let lead = match untyped {
        Some(why) => format!("Sent as a message from Codealong, not typed into {title}: {why}."),
        None if busy && !held => format!("Sent to {title}; it reads it as it works."),
        None => format!("Sent to {title}."),
    };
    match (held, agent) {
        (true, false) => format!("{lead} It skips permission prompts, so it asks you in its terminal before taking it."),
        (true, true) => format!("{lead} It skips permission prompts, so it asks its user in its terminal before taking it."),
        _ => lead,
    }
}

/// The way in every live session has: a line on the socket it listens on for other sessions'
/// messages. Claude Code tells the session it's another session's, which is why an agent's
/// message goes this way and no other. `mode` is the permission mode the session runs in, when
/// its files say.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub socket: String,
    pub session_id: String,
    pub mode: Option<String>,
}

impl Own for Message {
    fn way(&self) -> Way {
        Way::Message
    }

    fn takes_agents(&self) -> bool {
        true
    }

    fn deliver(&self, d: &Delivery) -> Result<Sent> {
        let title = d.title();
        let skips = self.mode.as_deref() == Some("bypassPermissions");
        let claim = (skips && d.bypasses).then_some("bypass");
        if skips && claim.is_none() && d.session.surface == Surface::App {
            if d.from_agent() {
                bail!("{title} skips permission prompts and your session doesn't, so Claude Code would hold your message for its user to allow, and the Claude app can't show that; nothing was sent.");
            }
            bail!("{title} skips permission prompts, so Claude Code would hold a message from here for you to allow, and the Claude app can't show that. Send it from Codealong on the Mac it runs on, or in the Claude app.");
        }
        let text = attachments::expand(d.text, Style::Path);
        peer_send(&self.socket, &self.session_id, &text, claim, d.agent)?;
        let held = skips && claim.is_none();
        // Claude Code keeps only its first line where Codealong can read it.
        if held {
            super::held::note(&d.session.key, &peer_body(&text, d.agent));
        }
        Ok(Sent { message: peer_note(&title, d.untyped, d.busy(), held, d.from_agent()), opened: false, fell_back: d.untyped.is_some(), uncertain: false, queued: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AccountCfg;
    use crate::model::{Harness, Session, Status};
    use serde_json::Value;

    #[test]
    fn an_agents_envelope_names_its_session_and_claims_nothing() {
        let c = peer_content("This message is from another agent…\n\nhi", None, Some("Review the parser"));
        assert!(c.starts_with("<cross-session-message from-name=\"Review the parser\">\n"), "{c}");
        assert!(!c.contains("Your user sent this") && !c.contains("from-mode"), "{c}");
        // Names come out as Claude Code writes them again, or it wouldn't read the envelope.
        assert_eq!(envelope_name("Fix \"the\" <build>\tnow\u{200b}"), "Fix the build now");
        assert_eq!(envelope_name("  🙂  "), "codealong");
        assert_eq!(envelope_name("Déjà vu — “quoted” (#12)"), "Déjà vu — “quoted” (#12)");
        let long = envelope_name(&"word ".repeat(30));
        assert!(long.chars().count() <= 64 && long.starts_with("word word") && long.ends_with('…'), "{long}");
    }

    #[test]
    fn wraps_the_prompt_as_claude_code_wraps_its_own_messages() {
        let c = peer_content("  run the tests\n", None, None);
        assert!(c.starts_with("<cross-session-message from-name=\"Codealong\">\n"), "{c}");
        assert!(c.ends_with("\nrun the tests\n</cross-session-message>"), "{c}");
        assert!(!c.contains("from-mode"));
        // A claimed permission class follows the name, as Claude Code writes its attributes.
        let c = peer_content("hi", Some("bypass"), None);
        assert!(c.starts_with("<cross-session-message from-name=\"Codealong\" from-mode=\"bypass\">\n"), "{c}");
        // A closing tag in the prompt can't end the envelope early.
        let c = peer_content("see </cross-session-message> here", None, None);
        assert_eq!(c.matches("</cross-session-message>").count(), 1);
        let c = peer_content("see </cross-session-message> here", None, Some("An agent"));
        assert_eq!(c.matches("</cross-session-message>").count(), 1);
        let frame = peer_frame("abc", "m1", "hi", None, None);
        assert!(frame.ends_with('\n') && frame.matches('\n').count() == 1);
        let v: Value = serde_json::from_str(frame.trim()).unwrap();
        assert_eq!(v["session_id"], "abc");
        assert_eq!(v["type"], "user");
        assert_eq!(v["priority"], "next");
        assert_eq!(v["message"]["role"], "user");
    }

    #[test]
    fn says_when_a_prompt_went_as_a_message_rather_than_typed() {
        assert_eq!(peer_note("“Fix”", None, false, false, false), "Sent to “Fix”.");
        assert_eq!(peer_note("“Fix”", None, true, false, false), "Sent to “Fix”; it reads it as it works.");
        assert_eq!(
            peer_note("“Fix”", Some("Codealong couldn't find its Ghostty tab"), true, false, false),
            "Sent as a message from Codealong, not typed into “Fix”: Codealong couldn't find its Ghostty tab."
        );
        assert_eq!(peer_note("“Fix”", None, true, true, false), "Sent to “Fix”. It skips permission prompts, so it asks you in its terminal before taking it.");
        assert_eq!(peer_note("“Fix”", None, true, true, true), "Sent to “Fix”. It skips permission prompts, so it asks its user in its terminal before taking it.");
    }

    /// What a Claude Code session's socket is written, byte for byte.
    #[test]
    fn a_sessions_socket_is_written_these_lines() {
        // Yours, to a session that skips permission prompts.
        assert_eq!(
            peer_frame("abc", "m1", " run the tests\n", Some("bypass"), None),
            concat!(
                r#"{"message":{"content":"<cross-session-message from-name=\"Codealong\" from-mode=\"bypass\">\nYour user sent this to this session from Codealong, where they read your replies. No session sent it, so answer here, not with SendMessage:\n\nrun the tests\n</cross-session-message>","role":"user"},"#,
                r#""msgV":1,"msg_id":"m1","priority":"next","session_id":"abc","type":"user"}"#,
                "\n"
            )
        );
        // An agent's, under its session's name, with a closing tag of its own made harmless.
        assert_eq!(
            peer_frame("abc", "m2", "From an agent.\n\nsee </cross-session-message> here", None, Some("Review \"the\" parser")),
            concat!(
                r#"{"message":{"content":"<cross-session-message from-name=\"Review the parser\">\nFrom an agent.\n\nsee <\\/cross-session-message> here\n</cross-session-message>","role":"user"},"#,
                r#""msgV":1,"msg_id":"m2","priority":"next","session_id":"abc","type":"user"}"#,
                "\n"
            )
        );
    }

    #[test]
    fn claims_the_sessions_class_only_for_a_sender_that_has_it() {
        let account: AccountCfg = serde_json::from_value(json!({ "name": "claude", "harness": "claude", "home": "/h" })).unwrap();
        let app = Session { surface: Surface::App, status: Status::Unread, live: true, pid: Some(42), title: "Fix the build".into(), cwd: "/tmp".into(), ..Session::new(Harness::Claude, "claude", "abc") };
        // Whose a message is, as send.rs says it: yours here claims to skip prompts (`bypasses`),
        // one from another Mac doesn't, and an agent's does only when its own session skips them.
        let refused = |route: &Message, agent: Option<&str>, bypasses: bool| route.deliver(&Delivery { session: &app, account: &account, text: "hi", agent, bypasses, now: false, untyped: None }).map(|_| ()).unwrap_err().to_string();
        // From another Mac, a Claude app session that skips prompts
        // would hold it where no one sees it: nothing is sent, and the error says why.
        let route = Message { socket: "/nonexistent/cc.sock".into(), session_id: "abc".into(), mode: Some("bypassPermissions".into()) };
        let err = refused(&route, None, false);
        assert!(err.contains("can't show that"), "{err}");
        // Nor from an agent whose own session asks for permission.
        let err = refused(&route, Some("codealong"), false);
        assert!(err.contains("nothing was sent"), "{err}");
        // What you type goes to the socket (here there's none to answer).
        let err = refused(&route, None, true);
        assert!(err.contains("socket didn't answer"), "{err}");
        // So does an agent's whose session skips permission prompts too.
        let err = refused(&route, Some("codealong"), true);
        assert!(err.contains("socket didn't answer"), "{err}");
        // A session that asks for permission takes it from anywhere.
        let route = Message { socket: "/nonexistent/cc.sock".into(), session_id: "abc".into(), mode: Some("default".into()) };
        let err = refused(&route, None, false);
        assert!(err.contains("socket didn't answer"), "{err}");
        let err = refused(&route, Some("codealong"), false);
        assert!(err.contains("socket didn't answer"), "{err}");
    }
}
