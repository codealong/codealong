//! What a message Codealong sent a session by itself comes to, in a line, for its card in the
//! session's conversation (CodealongCard.tsx), which a click opens to the whole message. Those
//! messages are written for the agent: session keys, commands to run, what not to do. The card
//! says what it's about (`About`), the PR it names, as a link, and what happened, in a few words,
//! each helper it names as a link too. Each module that writes such messages reads its own back
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
    /// What happened, in a few words a part, after the PR where there's one: each of a PR's
    /// happenings, each helper's news.
    pub parts: Vec<Part>,
}

/// One thing a message tells of.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    /// The session it's about (a helper of the one it went to), where it names one: its card
    /// names it as a link, before `line`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<Named>,
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

/// The longest a part gets, in characters: the card shows one line, cut short where it's narrow.
const MOST: usize = 200;

impl Glance {
    /// A glance of `about` with one part, `line`, about no session.
    pub fn line(about: About, line: &str) -> Glance {
        Glance { about, pr: None, parts: vec![Part { session: None, line: cut(line) }] }
    }

    /// The whole of it in words, as the card reads: each session by its title, in quotes.
    #[cfg(test)]
    pub fn words(&self) -> String {
        let part = |p: &Part| p.session.as_ref().map_or_else(|| p.line.clone(), |n| format!("“{}” {}", n.title, p.line));
        self.parts.iter().map(part).collect::<Vec<_>>().join(" · ")
    }
}

/// What a message Codealong sent by itself (its text past the line that says so) comes to. One
/// that resumed its session to say it starts with a line saying so, which the card leaves out.
pub fn of(text: &str) -> Glance {
    let text = text.trim();
    let text = text.strip_prefix(crate::send::RESUMED_TO_TELL).map_or(text, str::trim_start);
    // A schedule's run first: Codealong's line is its last paragraph, where none of the others'
    // is, and its prompt, before that, may start as any of theirs does.
    crate::schedules::runner::glance(text)
        .or_else(|| crate::nudges::glance(text))
        .or_else(|| crate::helpers::glance(text))
        .or_else(|| crate::autoreview::glance(text))
        .or_else(|| crate::reconnect::glance(text))
        .unwrap_or_else(|| Glance::line(About::Other, &first_sentence(text)))
}

/// The first sentence of `text`'s first line with words on it.
fn first_sentence(text: &str) -> &str {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    line.find(". ").map_or(line, |at| &line[..=at])
}

/// `s` on one line, cut short past `MOST` characters.
pub fn cut(s: &str) -> String {
    crate::util::truncate(&s.split_whitespace().collect::<Vec<_>>().join(" "), MOST)
}

/// `text` as an item of a message's list: "- " and `text`, its further lines indented under it
/// (blank ones left blank), so that only an item starts a line with "- ", whatever a comment or
/// a question it quotes has in it, and its reader can tell one item from the next.
pub fn item(text: &str) -> String {
    let lines: Vec<String> = text.split('\n').enumerate().map(|(n, l)| if n == 0 || l.is_empty() { l.to_string() } else { format!("  {l}") }).collect();
    format!("- {}", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_other_message_is_its_first_sentence_on_one_line() {
        assert_eq!(of("Check the deploy. Then tell me.").words(), "Check the deploy.");
        assert_eq!(of("\n\nReview pull request #7 in o/r\n\nIts diff is in pr-7.diff.").words(), "Review pull request #7 in o/r");
        assert_eq!(of("Check the deploy.").about, About::Other);
        let long = "word ".repeat(80);
        let line = of(&long).words();
        assert!(line.chars().count() == MOST && line.ends_with('…'), "{line}");
    }

    #[test]
    fn an_items_further_lines_go_under_it() {
        assert_eq!(item("ana commented: “Two:\n- one\n\n- two”"), "- ana commented: “Two:\n  - one\n\n  - two”");
        assert_eq!(item("ken approved it."), "- ken approved it.");
    }

    /// Codealong resuming a stopped session to tell it something says so first (send.rs
    /// `tell_waking`); the card is about what it came to tell.
    #[test]
    fn one_that_resumed_its_session_is_about_what_it_tells() {
        let news = "On your PR #1 “Fix it” (https://github.com/o/r/pull/1):\n- ken approved it.\nIt's ready to merge.";
        let resumed = format!("{}\n\n{news}", crate::send::RESUMED_TO_TELL);
        assert_eq!(of(&resumed), of(news));
        assert_eq!(of(&resumed).about, About::Pr);
    }
}
