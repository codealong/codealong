//! What a message Codealong sent a session by itself comes to, in a line, for its card in the
//! session's conversation, which opens to the whole message. Those messages are written for the
//! agent: session keys, commands to run, what not to do. The line says what the message is about
//! (`About`), the PR it names, as a link, and what happened, in a few words, each helper it names
//! as a link too. The engine reads its own messages back into these (its glance.rs) and sends them
//! on a conversation's items (`api::Item::glance`), so every device draws the same card.
//!
//! A device of another build reads them too: anything a newer Mac adds is optional, and a kind of
//! `About` it doesn't know reads as `Other`.

use serde::{Deserialize, Serialize};

/// A message of Codealong's, in a line.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Glance {
    #[serde(default)]
    pub about: About,
    /// The PR it's about, where it names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr: Option<Pr>,
    /// What happened, in a few words a part, after the PR where there's one: each of a PR's
    /// happenings, each helper's news.
    #[serde(default)]
    pub parts: Vec<Part>,
}

/// One thing a message tells of.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    /// The session it's about (a helper of the one it went to), where it names one: its card
    /// names it as a link, before `line`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<Named>,
    pub line: String,
}

/// What a message of Codealong's is about, for its card's icon.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum About {
    /// What happened on a PR.
    Pr,
    /// A helper's news.
    Helper,
    /// A schedule's run.
    Schedule,
    /// A review handed back.
    Review,
    /// Carrying on after what stopped its last turn.
    CarryOn,
    /// Anything else, and what a newer Mac says that this build doesn't know.
    #[default]
    #[serde(other)]
    Other,
}

/// A PR a message names: its number and title, and its address where the message gives it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pr {
    pub number: u64,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// A session a message names, by its key on the Mac that sent the message, and its title.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Named {
    pub key: String,
    pub title: String,
}

impl Glance {
    /// Its parts in words, as its card reads them after the PR: each session by its title, in
    /// quotes, before its words.
    pub fn words(&self) -> String {
        let part = |p: &Part| p.session.as_ref().map_or_else(|| p.line.clone(), |n| format!("“{}” {}", n.title, p.line));
        self.parts.iter().map(part).collect::<Vec<_>>().join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Mac of another build: one that leaves out what's optional, and one that says what this
    /// build doesn't know.
    #[test]
    fn reads_what_other_builds_write() {
        let bare: Glance = serde_json::from_value(serde_json::json!({ "about": "helper", "parts": [{ "line": "ended its turn" }] })).unwrap();
        assert_eq!(bare, Glance { about: About::Helper, pr: None, parts: vec![Part { session: None, line: "ended its turn".into() }] });
        let newer: Glance = serde_json::from_value(serde_json::json!({ "about": "meeting", "parts": [], "later": 1 })).unwrap();
        assert_eq!(newer.about, About::Other);
        let empty: Glance = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(empty, Glance::default());
        let carry: Glance = serde_json::from_value(serde_json::json!({ "about": "carry-on", "parts": [] })).unwrap();
        assert_eq!(carry.about, About::CarryOn);
    }

    #[test]
    fn its_words_are_each_part() {
        let helper = Part { session: Some(Named { key: "claude:work:k".into(), title: "Check staging".into() }), line: "ended its turn".into() };
        let news = Part { session: None, line: "ana approved it".into() };
        assert_eq!(Glance { about: About::Helper, pr: None, parts: vec![helper, news] }.words(), "“Check staging” ended its turn · ana approved it");
    }
}
