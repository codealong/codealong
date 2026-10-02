//! Messages between Claude Code sessions: the envelope one goes in, and the socket a live
//! session listens on for them (`messagingSocketPath`).

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use serde_json::json;

/// The sender's name on what you send, and the line its text starts with. The session's
/// transcript keeps both (conversation.rs, `sent_from_codealong`), which is how its panel knows the
/// message was yours.
pub const FROM_CODEALONG: &str = "Codealong";
pub const FROM_YOU: &str = "Your user sent this to this session from Codealong:";

/// The prompt as a cross-session message. The envelope is the one Claude Code writes for its
/// own sends (`<cross-session-message from-name="…">`, the body on lines of its own), so the
/// session shows who it's from: Codealong for what you sent, or `from`, an agent's session, whose
/// text carries its own byline. `claim` is the permission class it claims (`bypass` or
/// `prompting`, `from-mode`), which Claude Code compares with the session's own: a session
/// that skips prompts holds a message that claims none, or another, for you to allow. The
/// attributes go in the order Claude Code writes them, since it reads the envelope only when
/// writing it again gives the same text.
pub fn peer_content(text: &str, claim: Option<&str>, from: Option<&str>) -> String {
    let (name, body) = match from {
        Some(name) => (envelope_name(name), text.trim().to_string()),
        None => (FROM_CODEALONG.to_string(), format!("{FROM_YOU}\n\n{}", text.trim())),
    };
    let body = body.replace("</cross-session-message", "<\\/cross-session-message");
    let mode = claim.map(|m| format!(" from-mode=\"{m}\"")).unwrap_or_default();
    format!("<cross-session-message from-name=\"{name}\"{mode}>\n{body}\n</cross-session-message>")
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
pub fn peer_send(socket: &str, session_id: &str, text: &str, claim: Option<&str>, from: Option<&str>) -> Result<()> {
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
pub fn peer_note(title: &str, untyped: Option<&str>, busy: bool, held: bool, agent: bool) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
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
                r#"{"message":{"content":"<cross-session-message from-name=\"Codealong\" from-mode=\"bypass\">\nYour user sent this to this session from Codealong:\n\nrun the tests\n</cross-session-message>","role":"user"},"#,
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
}
