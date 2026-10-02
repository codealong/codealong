//! Sessions tied to PRs other than by branch: the ones whose own records show they made or
//! changed a PR, and the ones linked or unlinked with `codealong pr link|unlink`. The links
//! are kept in `pr-links.json` next to the config; the command writes it under a lock and the
//! engine watches it.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::support_dir;
use crate::store;

pub fn path() -> PathBuf {
    support_dir().join("pr-links.json")
}

/// What the file holds, for the error when it doesn't read as that.
const WHAT: &str = "a list of links";

/// A link made or taken away by hand. An unlink (`linked: false`) keeps the session off the
/// PR, however it was matched.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PrLink {
    /// `owner/name#number`.
    pub pr: String,
    /// The session's key.
    pub session: String,
    pub linked: bool,
    pub at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct LinkStore {
    #[serde(default)]
    pub links: Vec<PrLink>,
}

impl LinkStore {
    /// Records a link or an unlink, replacing any earlier one for the same PR and session.
    pub fn set(&mut self, pr: &str, session: &str, linked: bool, at: i64) -> bool {
        let pr = pr_key(pr);
        if self.links.iter().any(|l| l.pr == pr && l.session == session && l.linked == linked) {
            return false;
        }
        self.links.retain(|l| !(l.pr == pr && l.session == session));
        self.links.push(PrLink { pr, session: session.to_string(), linked, at });
        true
    }
}

/// The store, or an empty one when there's no file. A file that doesn't parse is an error, so
/// it's never overwritten.
pub fn load() -> Result<LinkStore> {
    Ok(store::read(&path(), WHAT)?.unwrap_or_default())
}

/// Reads the store under a lock on `pr-links.json.lock`, lets `f` change it, and saves it
/// through a temporary file when `f` says it changed.
pub fn change(f: impl FnOnce(&mut LinkStore) -> bool) -> Result<LinkStore> {
    store::change(&path(), WHAT, f)
}

/// How PR keys compare: GitHub's owner and repo names ignore case.
pub fn pr_key(pr: &str) -> String {
    pr.to_ascii_lowercase()
}

/// `owner/name#number` from a PR's URL (`https://github.com/owner/name/pull/number`, with
/// anything after it).
pub fn from_url(url: &str) -> Option<String> {
    let rest = &url[url.find("github.com/")? + "github.com/".len()..];
    let mut parts = rest.splitn(4, '/');
    let (owner, name, pull, tail) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    let number: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
    (pull == "pull" && !owner.is_empty() && !name.is_empty() && !number.is_empty()).then(|| format!("{owner}/{name}#{number}"))
}

/// The PRs a log's lines show the session made or changed, each with when (the line's time).
/// Each harness knows the marks its own records leave (`Adapter::pr_made`): Claude Code's on a
/// command's result, Codex's on the command or on its app's `attach_artifact` tool. Only lines
/// with one of those marks are parsed.
pub fn pr_marks(text: &str) -> Vec<(String, i64)> {
    text.lines().filter_map(line_mark).collect()
}

pub fn line_mark(line: &str) -> Option<(String, i64)> {
    let (url, at) = crate::harness::all().find_map(|a| a.pr_made(line))?;
    Some((pr_key(&from_url(&url)?), at))
}

/// Text a log line has when it may hold a PR mark (`line_mark`), every harness's: the engine
/// decodes only lines with one of these, as it reads a whole log once for everything it looks
/// for.
pub fn needles() -> impl Iterator<Item = &'static str> {
    crate::harness::all().flat_map(|a| a.pr_needles().iter().copied())
}

/// PR key → session keys, from what each session's records show (`marks`: session key → PR
/// keys) and the links made by hand, and the pairs unlinked by hand.
pub fn resolve(marks: &HashMap<String, Vec<String>>, store: &LinkStore) -> (HashMap<String, Vec<String>>, std::collections::HashSet<(String, String)>) {
    let mut worked: HashMap<String, Vec<String>> = HashMap::new();
    let mut add = |pr: &str, s: &str| {
        let v = worked.entry(pr_key(pr)).or_default();
        if !v.iter().any(|k| k == s) {
            v.push(s.to_string());
        }
    };
    for (s, prs) in marks {
        for pr in prs {
            add(pr, s);
        }
    }
    let mut unlinked = std::collections::HashSet::new();
    for l in &store.links {
        if l.linked {
            add(&l.pr, &l.session);
        } else {
            unlinked.insert((pr_key(&l.pr), l.session.clone()));
        }
    }
    (worked, unlinked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_pr_links() {
        assert_eq!(from_url("https://github.com/sprucehq/spark/pull/1057").as_deref(), Some("sprucehq/spark#1057"));
        assert_eq!(from_url("https://github.com/o/r/pull/12/files#diff").as_deref(), Some("o/r#12"));
        assert_eq!(from_url("https://github.com/o/r/pull/new/branch"), None);
        assert_eq!(from_url("https://github.com/o/r/issues/3"), None);
    }

    #[test]
    fn finds_the_prs_a_session_made_or_changed() {
        let claude = r#"{"type":"user","timestamp":"1970-01-01T00:00:01.000Z","toolUseResult":{"stdout":"https://github.com/SpruceHQ/spark/pull/1057","gitOperation":{"pr":{"number":1057,"url":"https://github.com/SpruceHQ/spark/pull/1057","action":"created"}}}}"#;
        let push = r#"{"type":"user","toolUseResult":{"stdout":"To github.com:o/r.git","gitOperation":{"push":{"branch":"x"}}}}"#;
        let codex = r#"{"timestamp":"1970-01-01T00:00:02.000Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","command":["/bin/zsh","-c","gh pr create --draft --title x"],"stdout":"Warning: 1 uncommitted change\nhttps://github.com/o/infra/pull/59\n"}}}"#;
        let codex_view = r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","command":["/bin/zsh","-c","gh pr view 58 --json url"],"stdout":"https://github.com/o/infra/pull/58\n"}}}"#;
        let attach = r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"McpToolCall","server":"codex_app","tool":"attach_artifact","arguments":{"artifact_type":"pull_request","url":"https://github.com/o/infra/pull/60"}}}}"#;
        let asked = r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"UserMessage","content":[{"type":"text","text":"run gh pr create, CommandExecution"}]}}}"#;
        let text = [claude, push, codex, codex_view, attach, asked].join("\n");
        assert_eq!(pr_marks(&text), vec![("sprucehq/spark#1057".to_string(), 1000), ("o/infra#59".to_string(), 2000), ("o/infra#60".to_string(), 0)]);
    }

    /// Only lines with one of a harness's marks are decoded.
    #[test]
    fn decodes_only_lines_with_a_harnesss_mark() {
        let mut needles: Vec<&str> = needles().collect();
        needles.sort();
        assert_eq!(needles, ["\"attach_artifact\"", "\"gitOperation\"", "gh pr create"]);
    }

    #[test]
    fn an_unlink_wins_over_the_records() {
        let marks = HashMap::from([("claude:a:1".to_string(), vec!["o/r#1".to_string()])]);
        let mut store = LinkStore::default();
        assert!(store.set("O/r#2", "claude:a:1", true, 1));
        assert!(!store.set("o/r#2", "claude:a:1", true, 2));
        assert!(store.set("o/r#1", "claude:a:1", false, 3));
        let (worked, unlinked) = resolve(&marks, &store);
        assert_eq!(worked["o/r#2"], vec!["claude:a:1".to_string()]);
        assert!(unlinked.contains(&("o/r#1".to_string(), "claude:a:1".to_string())));
        // Linking again replaces the unlink.
        assert!(store.set("o/r#1", "claude:a:1", true, 4));
        assert_eq!(store.links.len(), 2);
    }
}
