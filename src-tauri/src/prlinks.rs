//! Sessions tied to PRs other than by branch: the ones whose own records show they made or
//! changed a PR, and the ones linked or unlinked with `codealong pr link|unlink`. The links
//! are kept in `pr-links.json` next to the config; the command writes it under a lock and the
//! engine watches it. The same file has the PRs whose primary session is told of what happens on
//! them (nudges.rs), which a PR's card and menu and `codealong pr nudge` turn on and off one by
//! one, and the sessions picked by hand as their PR's primary (`PrLink::primary`).

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
    /// The session was picked as the PR's primary, the one told of what happens on it
    /// (`codealong pr nudge`), over the one whose records show it opened the PR. One link of a
    /// PR at most. A build from before this field drops it when it next writes the file.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub primary: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LinkStore {
    #[serde(default)]
    pub links: Vec<PrLink>,
    /// The PRs turned on for telling their primary session what happens on them (nudges.rs), as
    /// `pr_key` writes them. A build from before this field drops it when it next writes the file.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nudges: Vec<String>,
    /// The PRs turned off for it, which keeps one of yours off while `[pull_requests]
    /// nudge_by_default` is on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nudges_off: Vec<String>,
}

impl LinkStore {
    /// Records a link or an unlink, replacing any earlier one for the same PR and session.
    pub fn set(&mut self, pr: &str, session: &str, linked: bool, at: i64) -> bool {
        let pr = pr_key(pr);
        if self.links.iter().any(|l| l.pr == pr && l.session == session && l.linked == linked) {
            return false;
        }
        self.links.retain(|l| !(l.pr == pr && l.session == session));
        self.links.push(PrLink { pr, session: session.to_string(), linked, at, primary: false });
        true
    }

    /// Turns telling a PR's primary session what happens on it on or off for this PR, whatever
    /// `[pull_requests] nudge_by_default` says. True when that changed the file.
    pub fn set_nudge(&mut self, pr: &str, on: bool) -> bool {
        let pr = pr_key(pr);
        let (add, remove) = if on { (&mut self.nudges, &mut self.nudges_off) } else { (&mut self.nudges_off, &mut self.nudges) };
        let removed = remove.contains(&pr);
        remove.retain(|k| *k != pr);
        let added = !add.contains(&pr);
        if added {
            add.push(pr);
        }
        added || removed
    }

    /// Picks the session as the PR's primary, in place of any picked before, and links it. True
    /// when that changed the file.
    pub fn set_primary(&mut self, pr: &str, session: &str, at: i64) -> bool {
        let pr = pr_key(pr);
        if self.picked(&pr) == Some(session) {
            return false;
        }
        for l in self.links.iter_mut().filter(|l| l.pr == pr) {
            l.primary = false;
        }
        self.links.retain(|l| !(l.pr == pr && l.session == session));
        self.links.push(PrLink { pr, session: session.to_string(), linked: true, at, primary: true });
        true
    }

    /// The session picked as the PR's primary (`pr` as `pr_key` writes it), if one was.
    pub fn picked(&self, pr: &str) -> Option<&str> {
        self.links.iter().find(|l| l.pr == pr && l.linked && l.primary).map(|l| l.session.as_str())
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

/// The PRs each session's records show it made or changed, by session key: each PR's key with
/// the time of its first mark, subagents' logs included.
pub type Marks = HashMap<String, Vec<(String, i64)>>;

/// Adds a mark to a session's, keeping the earliest time for a PR it has already. True when that
/// changed them.
pub fn add_mark(have: &mut Vec<(String, i64)>, pr: String, at: i64) -> bool {
    match have.iter_mut().find(|(k, _)| *k == pr) {
        Some((_, was)) if mark_order(at) < mark_order(*was) => {
            *was = at;
            true
        }
        Some(_) => false,
        None => {
            have.push((pr, at));
            true
        }
    }
}

/// How marks' times sort, earliest first: a line without one (0) after any that has.
pub fn mark_order(at: i64) -> (bool, i64) {
    (at == 0, at)
}

/// PR key → session keys, from what each session's records show and the links made by hand;
/// the pairs unlinked by hand; and PR key → the sessions whose records show it, each with the
/// time of its first mark, which says who opened it.
pub fn resolve(marks: &Marks, store: &LinkStore) -> (HashMap<String, Vec<String>>, std::collections::HashSet<(String, String)>, HashMap<String, Vec<(String, i64)>>) {
    let mut worked: HashMap<String, Vec<String>> = HashMap::new();
    let mut made: HashMap<String, Vec<(String, i64)>> = HashMap::new();
    let mut add = |pr: &str, s: &str| {
        let v = worked.entry(pr_key(pr)).or_default();
        if !v.iter().any(|k| k == s) {
            v.push(s.to_string());
        }
    };
    for (s, prs) in marks {
        for (pr, at) in prs {
            add(pr, s);
            made.entry(pr_key(pr)).or_default().push((s.clone(), *at));
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
    (worked, unlinked, made)
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
        // Claude Code's own link, written once a command made a PR however it did (`gh api`).
        let linked = r#"{"type":"pr-link","sessionId":"9793436a","prNumber":83,"prUrl":"https://github.com/northwindai/osprey/pull/83","prRepository":"northwindai/osprey","timestamp":"1970-01-01T00:00:03.000Z"}"#;
        let push = r#"{"type":"user","toolUseResult":{"stdout":"To github.com:o/r.git","gitOperation":{"push":{"branch":"x"}}}}"#;
        let codex = r#"{"timestamp":"1970-01-01T00:00:02.000Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","command":["/bin/zsh","-c","gh pr create --draft --title x"],"stdout":"Warning: 1 uncommitted change\nhttps://github.com/o/infra/pull/59\n"}}}"#;
        let codex_view = r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","command":["/bin/zsh","-c","gh pr view 58 --json url"],"stdout":"https://github.com/o/infra/pull/58\n"}}}"#;
        let attach = r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"McpToolCall","server":"codex_app","tool":"attach_artifact","arguments":{"artifact_type":"pull_request","url":"https://github.com/o/infra/pull/60"}}}}"#;
        let asked = r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"UserMessage","content":[{"type":"text","text":"run gh pr create, CommandExecution"}]}}}"#;
        let text = [claude, push, codex, codex_view, attach, asked, linked].join("\n");
        assert_eq!(pr_marks(&text), vec![("sprucehq/spark#1057".to_string(), 1000), ("o/infra#59".to_string(), 2000), ("o/infra#60".to_string(), 0), ("northwindai/osprey#83".to_string(), 3000)]);
    }

    /// Only lines with one of a harness's marks are decoded.
    #[test]
    fn decodes_only_lines_with_a_harnesss_mark() {
        let mut needles: Vec<&str> = needles().collect();
        needles.sort();
        assert_eq!(needles, ["\"attach_artifact\"", "\"gitOperation\"", "\"pr-link\"", "gh pr create"]);

    }

    #[test]
    fn an_unlink_wins_over_the_records() {
        let marks = HashMap::from([("claude:a:1".to_string(), vec![("o/r#1".to_string(), 5)])]);
        let mut store = LinkStore::default();
        assert!(store.set("O/r#2", "claude:a:1", true, 1));
        assert!(!store.set("o/r#2", "claude:a:1", true, 2));
        assert!(store.set("o/r#1", "claude:a:1", false, 3));
        let (worked, unlinked, made) = resolve(&marks, &store);
        assert_eq!(made["o/r#1"], vec![("claude:a:1".to_string(), 5)]);
        assert_eq!(worked["o/r#2"], vec!["claude:a:1".to_string()]);
        assert!(unlinked.contains(&("o/r#1".to_string(), "claude:a:1".to_string())));
        // Linking again replaces the unlink.
        assert!(store.set("o/r#1", "claude:a:1", true, 4));
        assert_eq!(store.links.len(), 2);
    }

    #[test]
    fn a_prs_nudge_is_turned_on_and_off_by_its_key_and_older_files_have_none() {
        let mut store: LinkStore = serde_json::from_str(r#"{"links":[]}"#).unwrap();
        assert!(store.nudges.is_empty());
        assert_eq!(serde_json::to_string(&store).unwrap(), r#"{"links":[]}"#);
        assert!(store.set_nudge("O/R#2", true));
        assert!(!store.set_nudge("o/r#2", true), "already on");
        assert_eq!(store.nudges, vec!["o/r#2".to_string()]);
        assert_eq!(serde_json::from_str::<LinkStore>(&serde_json::to_string(&store).unwrap()).unwrap(), store);
        // Off is kept too, for a PR the setting would otherwise turn on.
        assert!(store.set_nudge("o/R#2", false));
        assert!(!store.set_nudge("o/r#2", false));
        assert!(store.nudges.is_empty());
        assert_eq!(serde_json::to_string(&store).unwrap(), r#"{"links":[],"nudgesOff":["o/r#2"]}"#);
        assert!(store.set_nudge("o/r#2", true));
        assert!(store.nudges_off.is_empty());
    }

    #[test]
    fn one_session_is_picked_as_a_prs_primary_and_an_unlink_takes_it_back() {
        let mut store = LinkStore::default();
        assert!(store.set("o/r#1", "a", true, 1));
        assert!(store.set_primary("O/r#1", "b", 2));
        assert!(!store.set_primary("o/r#1", "b", 3), "already");
        assert_eq!(store.picked("o/r#1"), Some("b"));
        assert!(store.set_primary("o/r#1", "a", 4));
        assert_eq!(store.picked("o/r#1"), Some("a"));
        assert_eq!(store.links.iter().filter(|l| l.primary).count(), 1);
        // Both stay linked.
        let (worked, _, _) = resolve(&HashMap::new(), &store);
        assert_eq!(worked["o/r#1"].len(), 2);
        assert!(store.set("o/r#1", "a", false, 5));
        assert_eq!(store.picked("o/r#1"), None);
        // A file from before has no `primary`, and a link that isn't one writes none.
        let old: PrLink = serde_json::from_str(r#"{"pr":"o/r#1","session":"a","linked":true,"at":1}"#).unwrap();
        assert!(!old.primary);
        assert!(!serde_json::to_string(&old).unwrap().contains("primary"));
    }

    #[test]
    fn a_mark_keeps_its_earliest_time_and_one_without_a_time_comes_last() {
        let mut have = Vec::new();
        assert!(add_mark(&mut have, "o/r#1".into(), 0));
        assert!(add_mark(&mut have, "o/r#1".into(), 9));
        assert!(add_mark(&mut have, "o/r#1".into(), 5));
        assert!(!add_mark(&mut have, "o/r#1".into(), 7));
        assert!(!add_mark(&mut have, "o/r#1".into(), 0));
        assert_eq!(have, vec![("o/r#1".to_string(), 5)]);
    }
}
