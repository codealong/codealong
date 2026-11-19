//! What a message Codealong sent a session by itself comes to, in a line, for its card in the
//! session's conversation (SessionThread.tsx `CodealongCard`), which a click opens to the whole
//! message. Those messages are written for the agent: session keys, commands to run, what not to
//! do. The card says what it's about (`About`), the PR or session it names, as a link, and what
//! happened, in a few words. Each module that writes such messages reads its own back
//! (`nudges::glance`, `helpers::glance`, `schedules::runner::glance`, `autoreview::glance`,
//! `reconnect::glance`), with tests that read back what it writes, so the two can't drift; any
//! other message is its first sentence.

use serde::Serialize;

/// A message of Codealong's, in a line.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Glance {
    pub about: About,
    /// The PR it's about, where it names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr: Option<Pr>,
    /// The session it's about (a helper of the one it went to), where it names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<Named>,
    /// What happened, in a few words, after the PR or session where there's one.
    pub line: String,
}

/// What a message of Codealong's is about, for its card's icon.
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum About {
    /// What happened on a PR (nudges.rs).
    Pr,
    /// A helper's news (helpers.rs).
    Helper,
    /// A schedule's run (schedules/runner.rs).
    Schedule,
    /// A review handed back (autoreview.rs).
    Review,
    /// Carrying on after what stopped its last turn (reconnect.rs).
    CarryOn,
    #[default]
    Other,
}

/// A PR a message names: its number and title, and its address where the message gives it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pr {
    pub number: u64,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// A session a message names, by its key and title.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Named {
    pub key: String,
    pub title: String,
}

/// The longest a line gets, in characters: the card shows one line, cut short where it's narrow.
const MOST: usize = 200;

/// What a message Codealong sent by itself (its text past the line that says so) comes to.
pub fn of(text: &str) -> Glance {
    let text = text.trim();
    crate::nudges::glance(text)
        .or_else(|| crate::helpers::glance(text))
        .or_else(|| crate::schedules::runner::glance(text))
        .or_else(|| crate::autoreview::glance(text))
        .or_else(|| crate::reconnect::glance(text))
        .unwrap_or_else(|| Glance { line: first_sentence(text), ..Glance::default() })
}

/// The first sentence of `text`'s first line with words on it, cut short past `MOST` characters.
pub fn first_sentence(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let sentence = line.find(". ").map_or(line, |at| &line[..=at]);
    cut(sentence)
}

/// `s` on one line, cut short past `MOST` characters.
pub fn cut(s: &str) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= MOST {
        return s;
    }
    let kept: String = s.chars().take(MOST - 1).collect();
    format!("{}…", kept.trim_end())
}

/// What's between `open` and `close` at the start of `s`, and what's after `close`: “a title”
/// from `“a title” (key)`.
pub fn between<'a>(s: &'a str, open: &str, close: &str) -> Option<(&'a str, &'a str)> {
    s.strip_prefix(open)?.split_once(close)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_other_message_is_its_first_sentence_on_one_line() {
        assert_eq!(of("Check the deploy. Then tell me.").line, "Check the deploy.");
        assert_eq!(of("\n\nReview pull request #7 in o/r\n\nIts diff is in pr-7.diff.").line, "Review pull request #7 in o/r");
        assert_eq!(of("Check the deploy.").about, About::Other);
        let long = "word ".repeat(80);
        let line = of(&long).line;
        assert!(line.chars().count() == MOST && line.ends_with('…'), "{line}");
    }
}
