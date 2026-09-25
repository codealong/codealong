//! Pull requests and checks through `gh api graphql`, one query per repo. Uses your existing
//! `gh` sign-in; Codealong never handles a GitHub token itself.

use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::model::{Checks, Merge, PrState};
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
    /// Asked only of open PRs.
    pub merge: Merge,
    pub author: Option<String>,
    pub base_ref: Option<String>,
    pub created_at: i64,
    pub additions: u64,
    pub deletions: u64,
    pub changed_files: u64,
    /// Your review is requested, directly or through a team.
    pub review_requested: bool,
}

/// One repo's PRs, and who `gh` is signed in as.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RepoPrs {
    pub login: Option<String>,
    pub prs: Vec<PrRaw>,
}

const QUERY: &str = r#"
query($owner: String!, $name: String!, $requested: String!) {
  viewer { login }
  repository(owner: $owner, name: $name) {
    open: pullRequests(states: OPEN, first: 100, orderBy: {field: UPDATED_AT, direction: DESC}) { nodes { ...pr mergeStateStatus } }
    done: pullRequests(states: [MERGED, CLOSED], first: 40, orderBy: {field: UPDATED_AT, direction: DESC}) { nodes { ...pr } }
  }
  requested: search(query: $requested, type: ISSUE, first: 50) { nodes { ... on PullRequest { number } } }
}
fragment pr on PullRequest {
  number title url state isDraft reviewDecision updatedAt createdAt mergedAt closedAt headRefName headRefOid baseRefName
  additions deletions changedFiles
  author { login }
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

pub fn fetch(owner: &str, name: &str) -> Result<RepoPrs> {
    let owner_arg = format!("owner={owner}");
    let name_arg = format!("name={name}");
    let query_arg = format!("query={QUERY}");
    let requested_arg = format!("requested=repo:{owner}/{name} is:pr is:open review-requested:@me");
    let out = procs::run(
        "gh",
        &["api", "graphql", "-f", &query_arg, "-f", &owner_arg, "-f", &name_arg, "-f", &requested_arg],
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

pub fn parse(json: &str) -> Result<RepoPrs> {
    let v: Value = serde_json::from_str(json)?;
    let repo = v.pointer("/data/repository").ok_or_else(|| anyhow!("no repository in response"))?;
    let requested: std::collections::HashSet<u64> = v
        .pointer("/data/requested/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|n| n.get("number").and_then(Value::as_u64))
        .collect();
    let mut prs = Vec::new();
    for key in ["open", "done"] {
        let Some(nodes) = repo.pointer(&format!("/{key}/nodes")).and_then(Value::as_array) else { continue };
        for n in nodes {
            if let Some(mut pr) = parse_pr(n) {
                pr.review_requested = pr.state.is_open() && requested.contains(&pr.number);
                prs.push(pr);
            }
        }
    }
    let login = v.pointer("/data/viewer/login").and_then(Value::as_str).map(String::from);
    Ok(RepoPrs { login, prs })
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
        merge: merge_of(s("mergeStateStatus")),
        author: n.pointer("/author/login").and_then(Value::as_str).map(String::from),
        base_ref: s("baseRefName").map(String::from),
        created_at: ts("createdAt").unwrap_or(0),
        additions: n.get("additions").and_then(Value::as_u64).unwrap_or(0),
        deletions: n.get("deletions").and_then(Value::as_u64).unwrap_or(0),
        changed_files: n.get("changedFiles").and_then(Value::as_u64).unwrap_or(0),
        review_requested: false,
    })
}

// ---- One PR, for its detail panel ----

const DETAIL_QUERY: &str = r#"
query($owner: String!, $name: String!, $number: Int!) {
  viewer { login }
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      bodyHTML mergeable
      commits { totalCount }
      reviews(last: 50) { nodes { id author { login } state submittedAt body url } }
      reviewRequests(first: 20) { nodes { requestedReviewer { __typename ... on User { login } ... on Team { name } } } }
      comments(last: 50) { nodes { id author { login } body createdAt url } }
      reviewThreads(last: 50) { nodes { isResolved path comments(first: 1) { nodes { id author { login } body createdAt url } } } }
    }
  }
}
"#;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrReview {
    pub author: String,
    /// `APPROVED`, `CHANGES_REQUESTED`, `COMMENTED` or `DISMISSED`.
    pub state: String,
    pub at: Option<i64>,
}

/// A comment on a PR: on the conversation, a review's own text, or the first comment of a
/// thread on the code.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrComment {
    pub id: String,
    pub author: String,
    /// As written, in Markdown.
    pub body: String,
    pub at: Option<i64>,
    pub url: Option<String>,
    /// The file a thread on the code is about.
    pub path: Option<String>,
    /// A thread on the code that's been resolved.
    pub resolved: bool,
    /// Written by whoever `gh` is signed in as.
    pub mine: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrFile {
    pub path: String,
    /// `added`, `modified`, `removed`, `renamed`…
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
    /// The unified diff; GitHub leaves it out for very large files.
    pub patch: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrDetail {
    /// The description as GitHub renders it, sanitized by GitHub.
    pub body_html: String,
    pub mergeable: Option<String>,
    pub commits: u64,
    /// Each reviewer's standing review: their last approval or change request, else comment.
    pub reviews: Vec<PrReview>,
    pub requested: Vec<String>,
    /// Comments with something written in them, newest first.
    pub comments: Vec<PrComment>,
    pub files: Vec<PrFile>,
    /// More files than the first page holds.
    pub more_files: bool,
}

/// A PR's description, reviews and changed files, with their diffs. Two requests, in parallel.
pub fn detail(slug: &str, number: u64) -> Result<PrDetail> {
    let (owner, name) = slug.split_once('/').ok_or_else(|| anyhow!("bad repo {slug}"))?;
    let (graph, files) = std::thread::scope(|scope| {
        let graph = scope.spawn(|| {
            let args = [format!("query={DETAIL_QUERY}"), format!("owner={owner}"), format!("name={name}")];
            let number_arg = format!("number={number}");
            procs::run("gh", &["api", "graphql", "-f", &args[0], "-f", &args[1], "-f", &args[2], "-F", &number_arg], None, Duration::from_secs(30))
        });
        let files = scope.spawn(|| procs::run("gh", &["api", &format!("repos/{slug}/pulls/{number}/files?per_page=100")], None, Duration::from_secs(30)));
        (graph.join(), files.join())
    });
    let graph = graph.map_err(|_| anyhow!("GitHub request failed"))?.map_err(|e| anyhow!("GitHub: {e}"))?;
    let files = files.map_err(|_| anyhow!("GitHub request failed"))?.map_err(|e| anyhow!("GitHub: {e}"))?;
    parse_detail(&graph, &files)
}

pub fn parse_detail(graph: &str, files: &str) -> Result<PrDetail> {
    let v: Value = serde_json::from_str(graph)?;
    let pr = v.pointer("/data/repository/pullRequest").ok_or_else(|| anyhow!("GitHub didn't return the PR"))?;
    let mut latest: Vec<PrReview> = Vec::new();
    for r in pr.pointer("/reviews/nodes").and_then(Value::as_array).into_iter().flatten() {
        let Some(author) = r.pointer("/author/login").and_then(Value::as_str) else { continue };
        let state = r.get("state").and_then(Value::as_str).unwrap_or("COMMENTED").to_string();
        let at = r.get("submittedAt").and_then(Value::as_str).and_then(parse_rfc3339_ms);
        match latest.iter_mut().find(|x| x.author == author) {
            // A later comment doesn't undo an approval or a change request.
            Some(x) if state == "COMMENTED" && x.state != "COMMENTED" => {}
            Some(x) => {
                x.state = state;
                x.at = at;
            }
            None => latest.push(PrReview { author: author.to_string(), state, at }),
        }
    }
    let requested = pr
        .pointer("/reviewRequests/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|n| {
            let r = n.get("requestedReviewer")?;
            r.get("login").or_else(|| r.get("name")).and_then(Value::as_str).map(String::from)
        })
        .collect();
    let viewer = v.pointer("/data/viewer/login").and_then(Value::as_str);
    let comment = |n: &Value, path: Option<String>, resolved: bool| -> Option<PrComment> {
        let body = n.get("body").and_then(Value::as_str)?.trim().to_string();
        if body.is_empty() {
            return None;
        }
        let author = n.pointer("/author/login").and_then(Value::as_str).unwrap_or("ghost").to_string();
        Some(PrComment {
            id: n.get("id").and_then(Value::as_str)?.to_string(),
            mine: viewer == Some(author.as_str()),
            author,
            body,
            at: n.get("createdAt").or_else(|| n.get("submittedAt")).and_then(Value::as_str).and_then(parse_rfc3339_ms),
            url: n.get("url").and_then(Value::as_str).map(String::from),
            path,
            resolved,
        })
    };
    let nodes = |at: &str| pr.pointer(at).and_then(Value::as_array).cloned().unwrap_or_default();
    let mut comments: Vec<PrComment> = nodes("/comments/nodes").iter().chain(nodes("/reviews/nodes").iter()).filter_map(|n| comment(n, None, false)).collect();
    for t in nodes("/reviewThreads/nodes") {
        let path = t.get("path").and_then(Value::as_str).map(String::from);
        let resolved = t.get("isResolved").and_then(Value::as_bool).unwrap_or(false);
        if let Some(first) = t.pointer("/comments/nodes/0") {
            comments.extend(comment(first, path, resolved));
        }
    }
    comments.sort_by(|a, b| b.at.cmp(&a.at).then(a.id.cmp(&b.id)));

    let list: Vec<Value> = serde_json::from_str(files).unwrap_or_default();
    let files: Vec<PrFile> = list
        .iter()
        .map(|f| PrFile {
            path: f.get("filename").and_then(Value::as_str).unwrap_or_default().to_string(),
            status: f.get("status").and_then(Value::as_str).unwrap_or("modified").to_string(),
            additions: f.get("additions").and_then(Value::as_u64).unwrap_or(0),
            deletions: f.get("deletions").and_then(Value::as_u64).unwrap_or(0),
            patch: f.get("patch").and_then(Value::as_str).map(String::from),
        })
        .collect();
    Ok(PrDetail {
        body_html: pr.get("bodyHTML").and_then(Value::as_str).unwrap_or_default().to_string(),
        mergeable: pr.get("mergeable").and_then(Value::as_str).map(String::from),
        commits: pr.pointer("/commits/totalCount").and_then(Value::as_u64).unwrap_or(0),
        reviews: latest,
        requested,
        comments,
        more_files: files.len() >= 100,
        files,
    })
}

/// Approves a PR as you, through `gh`.
pub fn approve(slug: &str, number: u64) -> Result<()> {
    procs::run("gh", &["pr", "review", &number.to_string(), "--approve", "-R", slug], None, Duration::from_secs(30))
        .map(|_| ())
        .map_err(|e| anyhow!("Couldn't approve #{number}: {e}"))
}

/// GitHub's `mergeStateStatus`. `HAS_HOOKS` is clean with server-side hooks to run on merge.
fn merge_of(status: Option<&str>) -> Merge {
    match status {
        Some("CLEAN") | Some("HAS_HOOKS") => Merge::Clean,
        Some("DIRTY") => Merge::Conflicts,
        Some("BEHIND") => Merge::Behind,
        Some("BLOCKED") | Some("DRAFT") => Merge::Blocked,
        Some("UNSTABLE") => Merge::Unstable,
        _ => Merge::Unknown,
    }
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
        let json = r#"{"data":{"viewer":{"login":"ghempton"},"requested":{"nodes":[{"number":1037}]},"repository":{
          "open":{"nodes":[{"number":1037,"title":"Let the agent add and caption project photos","url":"https://github.com/northwindai/atlas/pull/1037","state":"OPEN","isDraft":false,"reviewDecision":"APPROVED","mergeStateStatus":"UNSTABLE","updatedAt":"2026-09-23T20:00:00Z","mergedAt":null,"closedAt":null,"headRefName":"quarry","headRefOid":"0ef4ed2","headRepositoryOwner":{"login":"ghempton"},
             "commits":{"nodes":[{"commit":{"statusCheckRollup":{"contexts":{"nodes":[
               {"__typename":"CheckRun","name":"server:test","status":"COMPLETED","conclusion":"SUCCESS"},
               {"__typename":"CheckRun","name":"docker-smoke","status":"COMPLETED","conclusion":"FAILURE"},
               {"__typename":"CheckRun","name":"lint","status":"IN_PROGRESS","conclusion":null},
               {"__typename":"CheckRun","name":"deploy","status":"COMPLETED","conclusion":"SKIPPED"},
               {"__typename":"StatusContext","context":"vercel","state":"SUCCESS"}]}}}}]}}]},
          "done":{"nodes":[{"number":1016,"title":"Prevent page shifts","url":"u","state":"MERGED","isDraft":false,"reviewDecision":"APPROVED","updatedAt":"2026-09-23T18:00:00Z","mergedAt":"2026-09-23T18:00:00Z","closedAt":"2026-09-23T18:00:00Z","headRefName":"app6","headRepositoryOwner":{"login":"ghempton"},"commits":{"nodes":[]}}]}}}}"#;
        let repo = parse(json).unwrap();
        let prs = repo.prs;
        assert_eq!(prs.len(), 2);
        assert_eq!(prs[0].state, PrState::Approved);
        assert_eq!(prs[0].head_owner.as_deref(), Some("ghempton"));
        assert_eq!(prs[0].head_oid.as_deref(), Some("0ef4ed2"));
        assert_eq!((prs[0].checks.passed, prs[0].checks.failed, prs[0].checks.pending, prs[0].checks.skipped), (2, 1, 1, 1));
        assert_eq!(prs[0].checks.failing, vec!["docker-smoke".to_string()]);
        assert_eq!(prs[0].merge, Merge::Unstable);
        assert_eq!(prs[1].state, PrState::Merged);
        assert_eq!(prs[1].merge, Merge::Unknown);
        assert!(prs[1].merged_at.is_some());
        assert_eq!(repo.login.as_deref(), Some("ghempton"));
        assert!(prs[0].review_requested);
        assert!(!prs[1].review_requested);
    }

    #[test]
    fn keeps_each_reviewers_standing_review() {
        let graph = r#"{"data":{"repository":{"pullRequest":{"bodyHTML":"<p>Hi</p>","mergeable":"MERGEABLE","commits":{"totalCount":3},
          "reviews":{"nodes":[
            {"author":{"login":"ken"},"state":"APPROVED","submittedAt":"2026-09-23T10:00:00Z"},
            {"author":{"login":"ken"},"state":"COMMENTED","submittedAt":"2026-09-23T11:00:00Z"},
            {"author":{"login":"ana"},"state":"COMMENTED","submittedAt":"2026-09-23T12:00:00Z"},
            {"author":{"login":"ana"},"state":"CHANGES_REQUESTED","submittedAt":"2026-09-23T13:00:00Z"}]},
          "reviewRequests":{"nodes":[{"requestedReviewer":{"__typename":"User","login":"ghempton"}},{"requestedReviewer":{"__typename":"Team","name":"platform"}}]}}}}}"#;
        let files = r#"[{"filename":"src/a.ts","status":"modified","additions":3,"deletions":1,"patch":"@@ -1 +1 @@"},{"filename":"big.lock","status":"modified","additions":900,"deletions":800}]"#;
        let d = parse_detail(graph, files).unwrap();
        let states: Vec<(&str, &str)> = d.reviews.iter().map(|r| (r.author.as_str(), r.state.as_str())).collect();
        assert_eq!(states, vec![("ken", "APPROVED"), ("ana", "CHANGES_REQUESTED")]);
        assert_eq!(d.requested, vec!["ghempton".to_string(), "platform".to_string()]);
        assert_eq!(d.files.len(), 2);
        assert!(d.files[1].patch.is_none());
        assert_eq!(d.commits, 3);
        assert!(!d.more_files);
        assert!(d.comments.is_empty());
    }

    #[test]
    fn reads_comments_reviews_and_threads_newest_first() {
        let graph = r#"{"data":{"viewer":{"login":"me"},"repository":{"pullRequest":{"bodyHTML":"","mergeable":null,"commits":{"totalCount":1},
          "reviews":{"nodes":[
            {"id":"R1","author":{"login":"ana"},"state":"CHANGES_REQUESTED","submittedAt":"2026-09-23T13:00:00Z","body":"Please add a test.","url":"u1"},
            {"id":"R2","author":{"login":"ken"},"state":"APPROVED","submittedAt":"2026-09-23T14:00:00Z","body":"","url":"u2"}]},
          "reviewRequests":{"nodes":[]},
          "comments":{"nodes":[{"id":"C1","author":{"login":"me"},"body":"Done, thanks","createdAt":"2026-09-23T15:00:00Z","url":"u3"}]},
          "reviewThreads":{"nodes":[{"isResolved":true,"path":"src/a.ts","comments":{"nodes":[{"id":"T1","author":{"login":"ana"},"body":"Why this?","createdAt":"2026-09-23T12:00:00Z","url":"u4"}]}}]}}}}}"#;
        let d = parse_detail(graph, "[]").unwrap();
        let got: Vec<(&str, bool, bool, Option<&str>)> = d.comments.iter().map(|c| (c.id.as_str(), c.mine, c.resolved, c.path.as_deref())).collect();
        // The approval with nothing written is left out.
        assert_eq!(got, vec![("C1", true, false, None), ("R1", false, false, None), ("T1", false, true, Some("src/a.ts"))]);
    }
}
