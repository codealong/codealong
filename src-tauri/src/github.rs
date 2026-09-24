//! Pull requests and checks through `gh api graphql`, one query per repo. Uses your existing
//! `gh` sign-in; Codealong never handles a GitHub token itself.

use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::model::{Checks, PrState};
use crate::procs;
use crate::util::parse_rfc3339_ms;

#[derive(Debug, Clone, PartialEq)]
pub struct PrRaw {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: PrState,
    pub head_ref: String,
    pub head_owner: Option<String>,
    /// The commit the PR's branch pointed at, to tell a merged PR from a reused branch.
    pub head_oid: Option<String>,
    pub updated_at: i64,
    pub merged_at: Option<i64>,
    pub closed_at: Option<i64>,
    pub checks: Checks,
}

const QUERY: &str = r#"
query($owner: String!, $name: String!) {
  repository(owner: $owner, name: $name) {
    open: pullRequests(states: OPEN, first: 100, orderBy: {field: UPDATED_AT, direction: DESC}) { nodes { ...pr } }
    done: pullRequests(states: [MERGED, CLOSED], first: 40, orderBy: {field: UPDATED_AT, direction: DESC}) { nodes { ...pr } }
  }
}
fragment pr on PullRequest {
  number title url state isDraft reviewDecision updatedAt mergedAt closedAt headRefName headRefOid
  headRepositoryOwner { login }
  commits(last: 1) { nodes { commit { statusCheckRollup { contexts(first: 100) { nodes {
    __typename
    ... on CheckRun { name status conclusion }
    ... on StatusContext { context state }
  } } } } } }
}
"#;

/// An organization's display name on GitHub (`northwindai` → `Northwind`). Personal accounts
/// keep their login, since a person's name makes an odd project name.
pub fn display_name(owner: &str) -> Option<String> {
    let jq = r#"if .type == "Organization" then (.name // "") else "" end"#;
    let out = procs::run("gh", &["api", &format!("users/{owner}"), "--jq", jq], None, Duration::from_secs(5)).ok()?;
    let name = out.trim().to_string();
    (!name.is_empty()).then_some(name)
}

pub fn fetch(owner: &str, name: &str) -> Result<Vec<PrRaw>> {
    let owner_arg = format!("owner={owner}");
    let name_arg = format!("name={name}");
    let query_arg = format!("query={QUERY}");
    let out = procs::run(
        "gh",
        &["api", "graphql", "-f", &query_arg, "-f", &owner_arg, "-f", &name_arg],
        None,
        Duration::from_secs(30),
    )
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("couldn't start") {
            anyhow!("GitHub: install the gh CLI and sign in with `gh auth login` to see PRs")
        } else if msg.contains("auth login") || msg.contains("401") {
            anyhow!("GitHub: sign in again with `gh auth login` to see PRs")
        } else {
            anyhow!("GitHub: couldn't load PRs for {owner}/{name}")
        }
    })?;
    parse(&out)
}

pub fn parse(json: &str) -> Result<Vec<PrRaw>> {
    let v: Value = serde_json::from_str(json)?;
    let repo = v.pointer("/data/repository").ok_or_else(|| anyhow!("no repository in response"))?;
    let mut out = Vec::new();
    for key in ["open", "done"] {
        let Some(nodes) = repo.pointer(&format!("/{key}/nodes")).and_then(Value::as_array) else { continue };
        for n in nodes {
            if let Some(pr) = parse_pr(n) {
                out.push(pr);
            }
        }
    }
    Ok(out)
}

fn parse_pr(n: &Value) -> Option<PrRaw> {
    let s = |k: &str| n.get(k).and_then(Value::as_str);
    let ts = |k: &str| s(k).and_then(parse_rfc3339_ms);
    let state = match (s("state")?, n.get("isDraft").and_then(Value::as_bool).unwrap_or(false), s("reviewDecision")) {
        ("MERGED", _, _) => PrState::Merged,
        ("CLOSED", _, _) => PrState::Closed,
        (_, true, _) => PrState::Draft,
        (_, _, Some("APPROVED")) => PrState::Approved,
        (_, _, Some("CHANGES_REQUESTED")) => PrState::Changes,
        _ => PrState::Open,
    };
    let contexts = n
        .pointer("/commits/nodes/0/commit/statusCheckRollup/contexts/nodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Some(PrRaw {
        number: n.get("number")?.as_u64()?,
        title: s("title")?.to_string(),
        url: s("url")?.to_string(),
        state,
        head_ref: s("headRefName")?.to_string(),
        head_owner: n.pointer("/headRepositoryOwner/login").and_then(Value::as_str).map(String::from),
        head_oid: s("headRefOid").map(String::from),
        updated_at: ts("updatedAt").unwrap_or(0),
        merged_at: ts("mergedAt"),
        closed_at: ts("closedAt"),
        checks: tally(&contexts),
    })
}

pub fn tally(contexts: &[Value]) -> Checks {
    let mut c = Checks::default();
    for ctx in contexts {
        let s = |k: &str| ctx.get(k).and_then(Value::as_str);
        match ctx.get("__typename").and_then(Value::as_str) {
            Some("CheckRun") => {
                let name = s("name").unwrap_or("check").to_string();
                if s("status") != Some("COMPLETED") {
                    c.pending += 1;
                    continue;
                }
                match s("conclusion") {
                    Some("SUCCESS") | Some("NEUTRAL") => c.passed += 1,
                    Some("SKIPPED") | Some("CANCELLED") | Some("STALE") => c.skipped += 1,
                    _ => {
                        c.failed += 1;
                        c.failing.push(name);
                    }
                }
            }
            Some("StatusContext") => match s("state") {
                Some("SUCCESS") => c.passed += 1,
                Some("PENDING") | Some("EXPECTED") => c.pending += 1,
                _ => {
                    c.failed += 1;
                    c.failing.push(s("context").unwrap_or("status").to_string());
                }
            },
            _ => {}
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_prs_and_checks() {
        let json = r#"{"data":{"repository":{
          "open":{"nodes":[{"number":1037,"title":"Let the agent add and caption project photos","url":"https://github.com/northwindai/atlas/pull/1037","state":"OPEN","isDraft":false,"reviewDecision":"APPROVED","updatedAt":"2026-09-23T20:00:00Z","mergedAt":null,"closedAt":null,"headRefName":"quarry","headRefOid":"0ef4ed2","headRepositoryOwner":{"login":"ghempton"},
             "commits":{"nodes":[{"commit":{"statusCheckRollup":{"contexts":{"nodes":[
               {"__typename":"CheckRun","name":"server:test","status":"COMPLETED","conclusion":"SUCCESS"},
               {"__typename":"CheckRun","name":"docker-smoke","status":"COMPLETED","conclusion":"FAILURE"},
               {"__typename":"CheckRun","name":"lint","status":"IN_PROGRESS","conclusion":null},
               {"__typename":"CheckRun","name":"deploy","status":"COMPLETED","conclusion":"SKIPPED"},
               {"__typename":"StatusContext","context":"vercel","state":"SUCCESS"}]}}}}]}}]},
          "done":{"nodes":[{"number":1016,"title":"Prevent page shifts","url":"u","state":"MERGED","isDraft":false,"reviewDecision":"APPROVED","updatedAt":"2026-09-23T18:00:00Z","mergedAt":"2026-09-23T18:00:00Z","closedAt":"2026-09-23T18:00:00Z","headRefName":"app6","headRepositoryOwner":{"login":"ghempton"},"commits":{"nodes":[]}}]}}}}"#;
        let prs = parse(json).unwrap();
        assert_eq!(prs.len(), 2);
        assert_eq!(prs[0].state, PrState::Approved);
        assert_eq!(prs[0].head_owner.as_deref(), Some("ghempton"));
        assert_eq!(prs[0].head_oid.as_deref(), Some("0ef4ed2"));
        assert_eq!((prs[0].checks.passed, prs[0].checks.failed, prs[0].checks.pending, prs[0].checks.skipped), (2, 1, 1, 1));
        assert_eq!(prs[0].checks.failing, vec!["docker-smoke".to_string()]);
        assert_eq!(prs[1].state, PrState::Merged);
        assert!(prs[1].merged_at.is_some());
    }
}
