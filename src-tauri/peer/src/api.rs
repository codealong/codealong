//! What a device asks another for, and what comes back, as types both sides share: the engine
//! answers with them (`peer_answer` in its lib.rs) and a device that only watches reads them
//! (client.rs). A conversation page is built by the engine's adapters as its `transcript::Page`,
//! with borrowed strings; `Page` here reads one back, and the engine's tests check the two agree.

use serde::{Deserialize, Serialize};

/// The requests, by name.
pub const SESSION_PAGE: &str = "session-page";
pub const SESSION_NEWER: &str = "session-newer";
pub const SESSION_PENDING: &str = "session-pending";
pub const SESSION_ANSWER: &str = "session-answer";
pub const SESSION_SEND: &str = "session-send";
pub const SESSION_ARCHIVE: &str = "session-archive";
pub const MARK_SEEN: &str = "mark-seen";
pub const SEARCH: &str = "search";
pub const START_OPTIONS: &str = "start-options";
pub const START: &str = "start";

/// Requests that act on a session: they need remote control on at the device asked.
pub const CONTROL: [&str; 3] = [SESSION_SEND, SESSION_ANSWER, START];

/// One thing said or done in a session, as a conversation page carries it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub at: i64,
    /// `you`, `agent`, `tool` (a call, with its output when the record has it there), `result`
    /// (a call's output, by `id`) or `note` (compaction, a message from another session…).
    pub kind: String,
    /// Prose for you, agent and note; for a tool, a one-line label such as "Bash · `cargo test`".
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// A tool's whole input: the command, the file, the arguments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default)]
    pub failed: bool,
    /// Where its line starts in the record (or its message number); with `n`, unique in the session.
    pub cursor: u64,
    pub n: u32,
}

/// A page of a session's conversation, oldest first.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub items: Vec<Item>,
    /// Where the next page back ends; none at the session's start.
    pub before: Option<u64>,
    /// Where reading on, for what comes next, starts.
    pub after: u64,
}

/// What a session waits on you for, when it's a tool call you can allow or deny.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    /// The request's own id: the tool call's (Claude Code) or the permission request's (OpenCode).
    /// An answer names it, so a request that changed meanwhile isn't answered by mistake.
    pub id: String,
    /// The tool call as a line, e.g. "Bash · `rm -rf build`".
    pub label: String,
    /// All of it: the command, the file and the change.
    pub input: Option<String>,
    /// Why Codealong can't answer it here, when it can't; the panel then offers to open it.
    pub cannot: Option<String>,
}

/// Your answer to a pending request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Answer {
    Allow,
    Deny,
}

/// How a prompt sent to a session went.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Sent {
    /// One sentence for the panel: where the prompt went, or what to do next.
    pub message: String,
    /// The session or its app was opened for you to send it there, rather than sent.
    pub opened: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_reads_back_what_it_wrote() {
        let page = Page {
            items: vec![
                Item { at: 1, kind: "you".into(), text: "Fix the build".into(), id: None, input: None, output: None, failed: false, cursor: 0, n: 0 },
                Item { at: 2, kind: "tool".into(), text: "Bash · `cargo test`".into(), id: Some("t1".into()), input: Some("cargo test".into()), output: Some("ok".into()), failed: false, cursor: 120, n: 1 },
            ],
            before: Some(0),
            after: 400,
        };
        let back: Page = serde_json::from_value(serde_json::to_value(&page).unwrap()).unwrap();
        assert_eq!(back, page);
        let json = serde_json::to_string(&page).unwrap();
        assert!(json.contains(r#""before":0"#) && !json.contains("\"input\":null"), "{json}");
        assert_eq!(serde_json::to_value(Answer::Allow).unwrap(), "allow");
    }
}
