//! The pages on the web each session made or showed you (`model::Artifact`): an artifact Claude
//! Code published, made from a type or opened. Each harness knows the marks its own records
//! leave (`Adapter::artifact_made`). The engine reads every session's records for them once as
//! it starts, its subagents' logs included, in the same pass as the PRs they made (prlinks.rs),
//! and the lines sessions write after that as they come (`Engine::check_new_lines`).

use std::collections::HashMap;

use crate::model::Artifact;

/// How many a session keeps, the latest: a long session can publish many, and each goes to every
/// window and device with the board. Across this Mac's transcripts on 2026-10-12, the most one
/// session had was six.
pub const KEPT: usize = 20;

/// Each session's, by its key, the latest first.
pub type Made = HashMap<String, Vec<Artifact>>;

/// Text a log line has when it may name an artifact, every harness's: the engine decodes only
/// lines with one of these, as it reads a whole log once for everything it looks for.
pub fn needles() -> impl Iterator<Item = &'static str> {
    crate::harness::all().flat_map(|a| a.artifact_needles().iter().copied())
}

/// The artifact a log line names, by whichever harness wrote it.
pub fn line_artifact(line: &str) -> Option<Artifact> {
    crate::harness::all().find_map(|a| a.artifact_made(line))
}

/// The artifacts a log's lines name, in the order they come.
pub fn in_text(text: &str) -> Vec<Artifact> {
    text.lines().filter_map(line_artifact).collect()
}

/// Adds what a line says of an artifact to a session's, which stay the latest first. One it has
/// already (by its URL) is taken as the later of the two says it, its title and what was done
/// with it included: a session's own log and its subagents' are read apart, and a scan's lines
/// can come after newer ones read as they were written. A line without a time counts as later
/// than one with the same time or none, as lines read in order are. True when that changed them.
pub fn add(have: &mut Vec<Artifact>, a: Artifact) -> bool {
    if let Some(i) = have.iter().position(|h| h.url == a.url) {
        if have[i] == a || have[i].at > a.at {
            return false;
        }
        have.remove(i);
    }
    let at = have.iter().position(|h| h.at <= a.at).unwrap_or(have.len());
    if at >= KEPT {
        return false;
    }
    have.insert(at, a);
    have.truncate(KEPT);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn made(url: &str, title: &str, note: &str, at: i64) -> Artifact {
        Artifact { url: url.into(), title: title.into(), note: Some(note.into()), at }
    }

    /// A Claude Code transcript's result line for an `Artifact` call, as 2.1.296 writes it.
    fn published(url: &str, title: &str, seq: u64, at: &str) -> String {
        let result = serde_json::json!({ "url": url, "path": "/private/tmp/x.html", "artifact_id": "226b", "title": title, "updated": seq > 1, "seq": seq, "version": "1790892882-e855" });
        serde_json::json!({ "type": "user", "timestamp": at, "message": { "role": "user", "content": [{ "type": "tool_result", "tool_use_id": "t1", "content": format!("Published … at {url}") }] }, "toolUseResult": result }).to_string()
    }

    #[test]
    fn finds_the_artifacts_a_log_names() {
        let first = published("https://claude.ai/artifact/Vuaw", "Security dashboard", 1, "1970-01-01T00:00:01.000Z");
        let again = published("https://claude.ai/artifact/Vuaw", "Security dashboard", 2, "1970-01-01T00:00:03.000Z");
        // A read names an artifact without making one, and a command that printed a result like
        // it (a `jq` over a transcript) isn't a call of the tool.
        let read = r#"{"type":"user","timestamp":"1970-01-01T00:00:04.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"…"}]},"toolUseResult":{"read":{"url":"https://claude.ai/artifact/8ZBq","title":"Class C"},"artifactRead":{"slug":"3d2e"}}}"#;
        let printed = r#"{"type":"user","timestamp":"1970-01-01T00:00:05.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t3","content":"…"}]},"toolUseResult":{"stdout":"{\"artifact_id\":\"1\",\"url\":\"https://claude.ai/artifact/X\"}"}}"#;
        let text = [first.as_str(), read, again.as_str(), printed].join("\n");
        assert_eq!(in_text(&text), vec![made("https://claude.ai/artifact/Vuaw", "Security dashboard", "Published · version 1", 1000), made("https://claude.ai/artifact/Vuaw", "Security dashboard", "Updated · version 2", 3000)]);
        let mut needles: Vec<&str> = needles().collect();
        needles.sort();
        assert_eq!(needles, ["\"artifact_id\"", "\"created_from_type\""]);
    }

    #[test]
    fn keeps_each_artifact_once_as_it_last_stood_the_latest_first() {
        let mut have = Vec::new();
        assert!(add(&mut have, made("a", "Dashboard", "Published · version 1", 10)));
        assert!(add(&mut have, made("b", "Register", "Published · version 1", 20)));
        // Published again, under a new title: it moves to the front as it stands now.
        assert!(add(&mut have, made("a", "Security dashboard", "Updated · version 2", 30)));
        assert_eq!(have, vec![made("a", "Security dashboard", "Updated · version 2", 30), made("b", "Register", "Published · version 1", 20)]);
        // An earlier line of it, read after (a subagent's log), changes nothing; nor does the
        // same line twice.
        assert!(!add(&mut have, made("a", "Dashboard", "Published · version 1", 10)));
        assert!(!add(&mut have, made("b", "Register", "Published · version 1", 20)));
        // One from earlier than the rest goes after them.
        assert!(add(&mut have, made("c", "Design system", "Created", 5)));
        assert_eq!(have.iter().map(|a| a.url.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
    }

    #[test]
    fn keeps_only_the_latest_few() {
        let mut have = Vec::new();
        for i in 0..KEPT as i64 + 3 {
            add(&mut have, made(&format!("u{i}"), "Page", "Published", 100 + i));
        }
        assert_eq!(have.len(), KEPT);
        assert_eq!(have[0].url, format!("u{}", KEPT + 2));
        // One older than every one kept isn't taken.
        assert!(!add(&mut have, made("old", "Page", "Published", 1)));
        assert_eq!(have.len(), KEPT);
    }
}
