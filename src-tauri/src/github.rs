//! Pull requests and checks through `gh api graphql`, one query per repo, and one search for the
//! PRs that wait on your review wherever they are. Uses your existing `gh` sign-in; Codealong
//! never handles a GitHub token itself.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::model::{Checks, Merge, PrState};
use crate::procs;
use crate::util::{now_ms, parse_rfc3339_ms};

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
    /// Who asked for it, who needn't be the author; None when the PR's last requests don't say.
    pub requested_by: Option<String>,
    /// When the latest request to you was made, when the PR's last requests say: what a review
    /// waits on you since, for the bar's queue (barfeed.rs).
    pub requested_at: Option<i64>,
}

/// One repo's PRs, and who `gh` is signed in as.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RepoPrs {
    pub login: Option<String>,
    pub prs: Vec<PrRaw>,
}

/// Open PRs anywhere on GitHub that wait on your review, each with its repo as GitHub names it
/// now, and who `gh` is signed in as.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Requested {
    pub login: Option<String>,
    pub prs: Vec<(String, PrRaw)>,
}

const QUERY: &str = r#"
query($owner: String!, $name: String!, $requested: String!) {
  viewer { login }
  repository(owner: $owner, name: $name) {
    open: pullRequests(states: OPEN, first: 100, orderBy: {field: UPDATED_AT, direction: DESC}) { nodes { ...pr mergeStateStatus ...asked } }
    done: pullRequests(states: [MERGED, CLOSED], first: 40, orderBy: {field: UPDATED_AT, direction: DESC}) { nodes { ...pr } }
  }
  requested: search(query: $requested, type: ISSUE, first: 50) { nodes { ... on PullRequest { number } } }
}
"#;

/// PRs that ask for your review, in every repo: the board reads a repo's PRs only while one of
/// its worktrees is on a branch of its own, so a request in a repo checked out only on its
/// default branch is found here.
const REQUESTED_QUERY: &str = r#"
query($requested: String!) {
  viewer { login }
  search(query: $requested, type: ISSUE, first: 50) { nodes { ... on PullRequest { ...pr mergeStateStatus ...asked repository { nameWithOwner } } } }
}
"#;

/// Whose reviews an open PR still waits on (`asked_of`), and who asked for the latest of them
/// (`asker_of`).
const ASKED_FIELDS: &str = r#"
fragment asked on PullRequest {
  reviewRequests(first: 20) { nodes { requestedReviewer { __typename ... on User { login } } } }
  timelineItems(itemTypes: [REVIEW_REQUESTED_EVENT], last: 10) { nodes { ... on ReviewRequestedEvent { createdAt actor { login } requestedReviewer { __typename ... on User { login } } } } }
}
"#;

/// What's read of each PR, for the board and for a link to one.
const PR_FIELDS: &str = r#"
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

/// What GitHub calls a repo now (`atlasai/atlas` → `northwindai/atlas`). A repo that was
/// renamed, or whose owner was renamed or changed, still answers to its old name, which older
/// checkouts' remotes keep: GitHub redirects it. None when GitHub didn't answer.
pub fn current_slug(slug: &str) -> Option<String> {
    let out = procs::run("gh", &["api", &format!("repos/{slug}"), "--jq", ".full_name"], None, Duration::from_secs(10)).ok()?;
    slug_of(&out)
}

fn slug_of(out: &str) -> Option<String> {
    let name = out.trim();
    let (owner, repo) = name.split_once('/')?;
    let part = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    (part(owner) && part(repo)).then(|| name.to_string())
}

/// A repo's PRs as GitHub answers (`QUERY`), for `parse`.
fn fetch_text(owner: &str, name: &str) -> std::result::Result<String, Failure> {
    let owner_arg = format!("owner={owner}");
    let name_arg = format!("name={name}");
    let query_arg = format!("query={QUERY}{PR_FIELDS}{ASKED_FIELDS}");
    let requested_arg = format!("requested=repo:{owner}/{name} is:pr is:open review-requested:@me");
    procs::run(
        "gh",
        &["api", "graphql", "-f", &query_arg, "-f", &owner_arg, "-f", &name_arg, "-f", &requested_arg],
        None,
        Duration::from_secs(30),
    )
    .map_err(|e| failed(&e, &format!("PRs for {owner}/{name}")))
}

/// The open PRs that wait on your review, wherever they are (`REQUESTED_QUERY`), as GitHub
/// answers, for `parse_requested`.
fn fetch_requested_text() -> std::result::Result<String, Failure> {
    let query_arg = format!("query={REQUESTED_QUERY}{PR_FIELDS}{ASKED_FIELDS}");
    let requested_arg = format!("requested={REQUESTED_SEARCH}");
    procs::run("gh", &["api", "graphql", "-f", &query_arg, "-f", &requested_arg], None, Duration::from_secs(30))
        .map_err(|e| failed(&e, "the PRs that wait on your review"))
}

const REQUESTED_SEARCH: &str = "is:pr is:open archived:false review-requested:@me sort:updated-desc";

/// What went wrong asking `gh`. The board says it in `Display`'s words, and the engine goes by
/// which it is: an account out of requests is left alone until its hour ends.
#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    /// `gh` couldn't be started: it isn't on this Mac.
    NotInstalled,
    /// `gh` isn't signed in, or GitHub turned its sign-in away.
    SignedOut,
    /// The account has no requests left this hour; when the hour ends, where GitHub said.
    RateLimited { resets: Option<i64> },
    /// Anything else, in the words the board says it in.
    Other(String),
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Failure::NotInstalled => "GitHub: install the gh CLI and sign in with `gh auth login` to see PRs",
            Failure::SignedOut => "GitHub: sign in again with `gh auth login` to see PRs",
            Failure::RateLimited { .. } => "GitHub: your account's rate limit is used up, so PRs wait until it resets. `gh` and your agents draw on the same limit.",
            Failure::Other(text) => text,
        })
    }
}

impl Failure {
    /// In a few words, for a notice's title.
    pub fn title(&self) -> &'static str {
        match self {
            Failure::RateLimited { .. } => "GitHub rate limit reached",
            Failure::NotInstalled => "GitHub CLI isn't installed",
            Failure::SignedOut => "Signed out of GitHub",
            Failure::Other(_) => "GitHub didn't answer",
        }
    }

    /// Whether it's GitHub saying the account has no requests left.
    pub fn rate_limited(&self) -> bool {
        matches!(self, Failure::RateLimited { .. })
    }
}

/// Why a `gh` call for `what` failed, from what `gh` said.
fn failed(e: &anyhow::Error, what: &str) -> Failure {
    let msg = e.to_string();
    if msg.contains("couldn't start") {
        Failure::NotInstalled
    } else if msg.contains("auth login") || msg.contains("401") {
        Failure::SignedOut
    } else if msg.to_lowercase().contains("rate limit") {
        Failure::RateLimited { resets: None }
    } else {
        Failure::Other(format!("GitHub: couldn't load {what}"))
    }
}

/// One PR, as the board reads PRs, for a link to one the board doesn't list; and who `gh` is
/// signed in as.
pub fn one(slug: &str, number: u64) -> Result<(PrRaw, Option<String>)> {
    let (owner, name) = slug.split_once('/').ok_or_else(|| anyhow!("bad repo {slug}"))?;
    let args = [format!("query={ONE_QUERY}{PR_FIELDS}"), format!("owner={owner}"), format!("name={name}"), format!("number={number}")];
    // Said where the link is, in a sentence of its own: nothing here is a notice.
    let out = procs::run("gh", &["api", "graphql", "-f", &args[0], "-f", &args[1], "-f", &args[2], "-F", &args[3]], None, Duration::from_secs(20)).map_err(|e| match failed(&e, "the PR") {
        Failure::NotInstalled => anyhow!("Install the gh CLI and sign in with `gh auth login` to see PRs."),
        Failure::SignedOut => anyhow!("Sign in again with `gh auth login` to see PRs."),
        _ if e.to_string().contains("Could not resolve") => anyhow!("GitHub has no PR #{number} in {slug} that you can see."),
        _ => anyhow!("GitHub didn't answer for {slug}#{number}."),
    })?;
    parse_one(&out)
}

const ONE_QUERY: &str = r#"
query($owner: String!, $name: String!, $number: Int!) {
  viewer { login }
  repository(owner: $owner, name: $name) { pullRequest(number: $number) { ...pr mergeStateStatus } }
}
"#;

pub fn parse_one(json: &str) -> Result<(PrRaw, Option<String>)> {
    let v: Value = serde_json::from_str(json)?;
    let n = v.pointer("/data/repository/pullRequest").filter(|n| !n.is_null()).ok_or_else(|| anyhow!("GitHub didn't return the PR"))?;
    let mut pr = parse_pr(n).ok_or_else(|| anyhow!("GitHub's answer for the PR didn't read"))?;
    // As on the board, where it's asked only of open PRs.
    if !pr.state.is_open() {
        pr.merge = Merge::Unknown;
    }
    Ok((pr, v.pointer("/data/viewer/login").and_then(Value::as_str).map(String::from)))
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
    let login = v.pointer("/data/viewer/login").and_then(Value::as_str);
    let mut prs = Vec::new();
    for key in ["open", "done"] {
        let Some(nodes) = repo.pointer(&format!("/{key}/nodes")).and_then(Value::as_array) else { continue };
        for n in nodes {
            if let Some(mut pr) = parse_pr(n) {
                pr.review_requested = pr.state.is_open() && asked_of(n, login, requested.contains(&pr.number));
                pr.requested_by = pr.review_requested.then(|| asker_of(n, login)).flatten();
                pr.requested_at = pr.review_requested.then(|| asked_at(n, login)).flatten();
                prs.push(pr);
            }
        }
    }
    Ok(RepoPrs { login: login.map(String::from), prs })
}

/// The search's PRs that still wait on your review: the search can go on listing one you've
/// reviewed, which the PR's own requests settle (`asked_of`).
pub fn parse_requested(json: &str) -> Result<Requested> {
    let v: Value = serde_json::from_str(json)?;
    let nodes = v.pointer("/data/search/nodes").and_then(Value::as_array).ok_or_else(|| anyhow!("no search in response"))?;
    let login = v.pointer("/data/viewer/login").and_then(Value::as_str);
    let prs = nodes
        .iter()
        .filter_map(|n| {
            let slug = n.pointer("/repository/nameWithOwner").and_then(Value::as_str)?;
            let mut pr = parse_pr(n)?;
            pr.review_requested = pr.state.is_open() && asked_of(n, login, true);
            pr.requested_by = asker_of(n, login);
            pr.requested_at = asked_at(n, login);
            pr.review_requested.then(|| (slug.to_string(), pr))
        })
        .collect();
    Ok(Requested { login: login.map(String::from), prs })
}

/// Whether a PR still waits on your review. GitHub's search is the only place that says a team
/// you're on was asked, but it can go on listing a PR for a while after you've reviewed it; the
/// PR's own review requests are as they are now. So a request to you is read from the PR, and
/// the search (`found`) counts only while the PR waits on a team, or on a reviewer you can't see.
fn asked_of(n: &Value, login: Option<&str>, found: bool) -> bool {
    let (Some(requests), Some(login)) = (n.pointer("/reviewRequests/nodes").and_then(Value::as_array), login) else { return found };
    requests.iter().any(|r| {
        let who = r.get("requestedReviewer").filter(|w| !w.is_null());
        match who.and_then(|w| w.get("__typename")).and_then(Value::as_str) {
            Some("User") => who.and_then(|w| w.get("login")).and_then(Value::as_str).is_some_and(|l| l.eq_ignore_ascii_case(login)),
            Some("Team") | None => found,
            _ => false,
        }
    })
}

/// Who asked for your review: whoever made the latest request to you, else the latest to a team
/// or to a reviewer you can't see, which is how a request through a team you're on reads. It's
/// often not the author: a teammate asks for a review of a bot's PR.
fn asker_of(n: &Value, login: Option<&str>) -> Option<String> {
    latest_request(n, login)?.pointer("/actor/login").and_then(Value::as_str).map(String::from)
}

/// When your review was last asked for (the request `asker_of` reads), for how long it has
/// waited on you.
fn asked_at(n: &Value, login: Option<&str>) -> Option<i64> {
    latest_request(n, login)?.get("createdAt").and_then(Value::as_str).and_then(parse_rfc3339_ms)
}

/// The latest request to you among the PR's last review requests, else the latest to a team or
/// to a reviewer you can't see, which is how a request through a team you're on reads.
fn latest_request<'a>(n: &'a Value, login: Option<&str>) -> Option<&'a Value> {
    let events = n.pointer("/timelineItems/nodes").and_then(Value::as_array)?;
    fn reviewer(e: &Value) -> Option<&Value> {
        e.get("requestedReviewer").filter(|w| !w.is_null())
    }
    let you = events.iter().rev().find(|e| {
        let asked = reviewer(e).and_then(|w| w.get("login")).and_then(Value::as_str);
        login.is_some_and(|me| asked.is_some_and(|l| l.eq_ignore_ascii_case(me)))
    });
    let team = || events.iter().rev().find(|e| reviewer(e).and_then(|w| w.get("__typename")).and_then(Value::as_str) != Some("User"));
    you.or_else(team)
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
        requested_by: None,
        requested_at: None,
    })
}

// ---- Asking only for what changed ----
//
// GitHub gives an account 5,000 GraphQL points an hour, and charges a query for the connections
// it could return, not for what comes back: a repo's PRs (`QUERY`) are 5 points whether it has a
// hundred open PRs or none. Asked of seven repos every minute that was 2,200 points an hour
// from the app alone, and 37 more for every `--snapshot` (each preview takes one), which left
// none for `gh` itself (2026-10-01). So a poll first takes a glance: one query for every repo,
// a point for each `GLANCE_REPOS` of them, that reads only what tells a change. A repo's PRs
// are read in full when its glance isn't the one they were read at, while its checks run, and
// every `RESYNC_MS`. Each answer is kept on disk with its glance, so the next engine to start
// (the app after a relaunch, a snapshot) reads from GitHub only what changed since.

/// Repos a glance asks about in one query. Their open PRs' merge states are what takes the
/// time (40 ms each; seven repos with 37 open PRs answered in 4 s), and GitHub gives a query 10.
const GLANCE_REPOS: usize = 8;
/// An answer whose glance hasn't changed is still read again this often, for what a glance
/// doesn't tell.
const RESYNC_MS: i64 = 10 * 60 * 1000;
/// While a repo's checks run, its PRs are read at every poll, which come a minute apart: how
/// many have passed changes without the glance saying so.
const BUSY_MS: i64 = 45 * 1000;
/// Checks still pending this long after their PR last changed are waiting on someone, or stuck.
const BUSY_FOR_MS: i64 = 60 * 60 * 1000;
/// With fewer points than this left in the hour, only a changed glance gets anything read: the
/// rest are for `gh`, which you and your agents use too.
pub const RESERVE: u64 = 500;
/// Kept answers that an engine which polls confirmed this lately are a snapshot's answers.
const FRESH: Duration = Duration::from_secs(150);

/// The key the search for PRs that wait on your review goes by, among stamps and kept answers.
/// A repo's is its slug, which has a slash.
pub const REQUESTED: &str = "requested";

/// What an answer was read at: its glance then, and when. The glance is empty for an answer
/// read when GitHub gave none.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stamp {
    pub glance: String,
    pub at: i64,
}

/// What a glance saw.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Glance {
    pub login: Option<String>,
    /// Each repo's mark by slug, which changes when its PRs do. None for a repo GitHub doesn't
    /// show you; a repo whose query failed isn't listed.
    pub repos: HashMap<String, Option<String>>,
    /// The mark of the search for PRs that wait on your review.
    pub requested: String,
    pub points: Option<Points>,
}

/// What a glance says of one repo.
enum Seen<'a> {
    /// Its mark, which changes when its PRs do.
    Mark(&'a str),
    /// GitHub doesn't show you the repo.
    Hidden,
    /// Nothing: its query failed, or GitHub gave no glance at all.
    Unknown,
}

impl Glance {
    fn seen(&self, slug: &str) -> Seen<'_> {
        match self.repos.get(slug) {
            Some(Some(mark)) => Seen::Mark(mark),
            Some(None) => Seen::Hidden,
            None => Seen::Unknown,
        }
    }
}

/// GraphQL points left in an hour of the account's, and when that hour ends, as an answer's
/// `X-RateLimit-*` headers say, a refusal's too. GitHub counts on more than one counter, each
/// with an hour of its own: one query in ten was answered from a second one that had hardly been
/// used (2026-10-01).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Points {
    pub left: u64,
    pub resets: i64,
}

/// What tells that a repo's PRs changed: an open PR's `updatedAt` moves with its commits,
/// comments, reviews and review requests, its checks' state and merge state move by themselves,
/// and anything that happens to a closed PR makes it the latest updated one.
const GLANCE_FIELDS: &str = r#"
fragment glance on Repository {
  open: pullRequests(states: OPEN, first: 100, orderBy: {field: UPDATED_AT, direction: DESC}) { nodes { number updatedAt headRefOid isDraft reviewDecision mergeStateStatus statusCheckRollup { state } } }
  done: pullRequests(states: [MERGED, CLOSED], first: 1, orderBy: {field: UPDATED_AT, direction: DESC}) { nodes { number updatedAt } }
}
"#;

fn glance_query(repos: usize) -> String {
    let vars: String = (0..repos).map(|i| format!(", $o{i}: String!, $n{i}: String!")).collect();
    let fields: String = (0..repos).map(|i| format!("  r{i}: repository(owner: $o{i}, name: $n{i}) {{ ...glance }}\n")).collect();
    // GitHub refuses a query with a fragment it doesn't use.
    let fragment = if repos > 0 { GLANCE_FIELDS } else { "" };
    format!(
        "query($requested: String!{vars}) {{\n  viewer {{ login }}\n{fields}  search(query: $requested, type: ISSUE, first: 50) {{ nodes {{ ... on PullRequest {{ number updatedAt repository {{ nameWithOwner }} }} }} }}\n}}\n{fragment}"
    )
}

/// A glance at every repo, `GLANCE_REPOS` to a query, asked at once. It fails only when no
/// query was answered.
pub fn glance(slugs: &[String]) -> std::result::Result<Glance, Failure> {
    let groups: Vec<&[String]> = if slugs.is_empty() { vec![slugs] } else { slugs.chunks(GLANCE_REPOS).collect() };
    let unanswered = || Failure::Other("GitHub: couldn't load PRs".into());
    let answers: Vec<std::result::Result<Glance, Failure>> = std::thread::scope(|scope| {
        let asked: Vec<_> = groups.iter().map(|group| scope.spawn(move || glance_some(group))).collect();
        asked.into_iter().map(|a| a.join().unwrap_or_else(|_| Err(unanswered()))).collect()
    });
    let mut seen: Option<Glance> = None;
    let mut failure = None;
    for answer in answers {
        match (answer, &mut seen) {
            (Ok(g), None) => seen = Some(g),
            (Ok(g), Some(all)) => {
                all.repos.extend(g.repos);
                all.points = [all.points, g.points].into_iter().flatten().min_by_key(|p| p.left);
            }
            (Err(e), _) => failure = failure.or(Some(e)),
        }
    }
    seen.ok_or_else(|| failure.unwrap_or_else(unanswered))
}

fn glance_some(slugs: &[String]) -> std::result::Result<Glance, Failure> {
    let mut fields = vec![format!("query={}", glance_query(slugs.len())), format!("requested={REQUESTED_SEARCH}")];
    for (i, slug) in slugs.iter().enumerate() {
        let (owner, name) = slug.split_once('/').unwrap_or((slug, ""));
        fields.push(format!("o{i}={owner}"));
        fields.push(format!("n{i}={name}"));
    }
    // With the answer's headers, which say how many points are left and when the hour ends.
    let mut args = vec!["api", "graphql", "-i"];
    for field in &fields {
        args.extend(["-f", field.as_str()]);
    }
    let o = procs::run_output("gh", &args, None, Duration::from_secs(30)).map_err(|e| failed(&e, "PRs"))?;
    let (points, body) = answer_parts(&o.out);
    // A repo GitHub doesn't show you fails the command, with the others answered. With nothing
    // answered, why is in what it printed: a refusal for want of points comes as an error in
    // the answer.
    match parse_glance(body, slugs) {
        Ok(seen) => Ok(Glance { points, ..seen }),
        Err(_) => Err(match failed(&anyhow!("gh {} {}", o.err.trim(), body.chars().take(400).collect::<String>()), "PRs") {
            // The refusal's own headers say when the hour ends.
            Failure::RateLimited { .. } => Failure::RateLimited { resets: points.map(|p| p.resets) },
            other => other,
        }),
    }
}

/// An answer printed with its headers (`gh api -i`): the points its `X-RateLimit-*` headers
/// tell of, and its body.
fn answer_parts(out: &str) -> (Option<Points>, &str) {
    if !out.starts_with("HTTP/") {
        return (None, out);
    }
    let (head, body) = out.split_once("\r\n\r\n").or_else(|| out.split_once("\n\n")).unwrap_or((out, ""));
    let header = |name: &str| {
        head.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
        })
    };
    let points = || Some(Points { left: header("x-ratelimit-remaining")?.parse().ok()?, resets: header("x-ratelimit-reset")?.parse::<i64>().ok()? * 1000 });
    (points(), body)
}

pub fn parse_glance(json: &str, slugs: &[String]) -> Result<Glance> {
    let v: Value = serde_json::from_str(json)?;
    let data = v.get("data").filter(|d| d.is_object()).ok_or_else(|| anyhow!("no data in response"))?;
    let search = data.get("search").filter(|s| !s.is_null()).ok_or_else(|| anyhow!("no search in response"))?;
    let login = data.pointer("/viewer/login").and_then(Value::as_str);
    // Whose glance it is counts: another sign-in sees other requests.
    let who = login.unwrap_or("");
    let waiting: Vec<(String, u64)> = search
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|n| Some((n.pointer("/repository/nameWithOwner")?.as_str()?.to_lowercase(), n.get("number")?.as_u64()?)))
        .collect();
    let repos = slugs
        .iter()
        .enumerate()
        .filter_map(|(i, slug)| {
            let repo = data.get(format!("r{i}").as_str())?;
            if repo.is_null() {
                return Some((slug.clone(), None));
            }
            // A PR that starts to wait on your review through a team changes only the search.
            let lower = slug.to_lowercase();
            let asked: Vec<String> = waiting.iter().filter(|(s, _)| *s == lower).map(|(_, n)| n.to_string()).collect();
            Some((slug.clone(), Some(mark(&[who, &repo.to_string(), &asked.join(",")]))))
        })
        .collect();
    Ok(Glance {
        login: login.map(String::from),
        repos,
        requested: mark(&[who, &search.to_string()]),
        points: None,
    })
}

/// A short mark of what a glance read. It's compared only with marks this build made: one
/// from another build that differs costs a read.
fn mark(parts: &[&str]) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    parts.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Whether a repo's checks are running: an open PR with some pending that changed lately.
pub fn busy(prs: &[PrRaw], now: i64) -> bool {
    prs.iter().any(|p| p.state.is_open() && p.checks.pending > 0 && now - p.updated_at < BUSY_FOR_MS)
}

/// Whether an answer read at `stamp` still stands when the glance is `glance`.
fn stands(stamp: &Stamp, glance: &str, busy: bool, thrifty: bool, now: i64) -> bool {
    stamp.glance == glance && (thrifty || now - stamp.at < if busy { BUSY_MS } else { RESYNC_MS })
}

/// What a poll goes by.
#[derive(Debug, Clone, Default)]
pub struct Ask {
    pub slugs: Vec<String>,
    /// What the asker's answers were read at, by slug, the search's under `REQUESTED`.
    pub have: HashMap<String, Stamp>,
    /// The repos whose checks are running (`busy`).
    pub busy: BTreeSet<String>,
    /// Read everything, whatever the glance says: after something you did on GitHub from here.
    pub all: bool,
    /// Few points are left this hour (`RESERVE`), by what the asker has heard: only a changed
    /// glance gets a repo read. The poll's own glance can say so too.
    pub thrifty: bool,
}

/// What a poll brings, as it comes.
pub enum Answer {
    /// First: the glance, or why GitHub gave none.
    Glance(std::result::Result<Glance, Failure>),
    /// A repo's PRs, read from GitHub or as kept, and what they were read at.
    Repo(String, std::result::Result<RepoPrs, Failure>, Option<Stamp>),
    Requested(std::result::Result<Requested, Failure>, Option<Stamp>),
}

/// One poll: a glance, then the PRs of each repo whose answer no longer stands, and the search
/// likewise, all at once. Nothing comes for an answer that stands.
pub fn poll(ask: &Ask, on: &(dyn Fn(Answer) + Sync)) {
    poll_in(&saved_dir(), ask, on)
}

fn poll_in(dir: &Path, ask: &Ask, on: &(dyn Fn(Answer) + Sync)) {
    let now = now_ms();
    let seen = glance(&ask.slugs);
    // Out of points: the reads would be refused too. An asker with nothing gets what's kept,
    // however old, and then hears why it's no newer.
    if seen.as_ref().is_err_and(Failure::rate_limited) {
        for key in ask.slugs.iter().map(String::as_str).chain([REQUESTED]).filter(|key| !ask.have.contains_key(*key)) {
            if let Some(answer) = saved(dir, key).and_then(|kept| kept_answer(key, kept)) {
                on(answer);
            }
        }
        return on(Answer::Glance(seen));
    }
    let glance = seen.as_ref().ok().cloned();
    on(Answer::Glance(seen));
    let glance = glance.as_ref();
    let short = glance.and_then(|g| g.points).is_some_and(|p| p.left < RESERVE);
    let ask = &Ask { thrifty: ask.thrifty || short, ..ask.clone() };
    std::thread::scope(|scope| {
        scope.spawn(move || {
            let mark = glance.map(|g| g.requested.as_str());
            let read = Reading { dir, ask, key: REQUESTED, mark, now, fetch: &fetch_requested_text, parse: &parse_requested, busy: &|_| false }.read();
            if let Some((result, stamp)) = read {
                on(Answer::Requested(result, stamp));
            }
        });
        for slug in &ask.slugs {
            scope.spawn(move || {
                let mark = match glance.map_or(Seen::Unknown, |g| g.seen(slug)) {
                    Seen::Hidden => return on(Answer::Repo(slug.clone(), Err(Failure::Other(format!("GitHub: couldn't load PRs for {slug}"))), None)),
                    Seen::Mark(mark) => Some(mark),
                    Seen::Unknown => None,
                };
                let Some((owner, name)) = slug.split_once('/') else { return };
                let read = Reading { dir, ask, key: slug, mark, now, fetch: &|| fetch_text(owner, name), parse: &parse, busy: &|repo: &RepoPrs| busy(&repo.prs, now) }.read();
                if let Some((result, stamp)) = read {
                    on(Answer::Repo(slug.clone(), result, stamp));
                }
            });
        }
    });
}

/// One answer a poll may bring: which it is, what's known of it, and how it's asked for and
/// read.
struct Reading<'a, T> {
    /// Where answers are kept.
    dir: &'a Path,
    ask: &'a Ask,
    /// A repo's slug, or `REQUESTED`.
    key: &'a str,
    /// Its glance, None when GitHub gave none.
    mark: Option<&'a str>,
    now: i64,
    /// Asks GitHub.
    fetch: &'a dyn Fn() -> std::result::Result<String, Failure>,
    parse: &'a dyn Fn(&str) -> Result<T>,
    /// Whether an answer's checks are running (`busy`).
    busy: &'a dyn Fn(&T) -> bool,
}

/// What a read brings: the answer or why there's none, and what it was read at.
type Brought<T> = (std::result::Result<T, Failure>, Option<Stamp>);

impl<T> Reading<'_, T> {
    /// None when what the asker has stands; else the kept answer when that stands, else
    /// GitHub's, asked now and kept.
    fn read(&self) -> Option<Brought<T>> {
        let Reading { dir, ask, key, mark, now, .. } = *self;
        if !ask.all {
            match (ask.have.get(key), mark) {
                (Some(have), Some(mark)) if stands(have, mark, ask.busy.contains(key), ask.thrifty, now) => {
                    confirm(dir, key);
                    return None;
                }
                // Without a glance, what's had stands until it would be read again anyway.
                (Some(have), None) if now - have.at < RESYNC_MS => return None,
                _ => {}
            }
            if let (Some(mark), Some(kept)) = (mark, saved(dir, key)) {
                if let Ok(answer) = (self.parse)(&kept.answer) {
                    if stands(&kept.stamp, mark, ask.busy.contains(key) || (self.busy)(&answer), ask.thrifty, now) {
                        confirm(dir, key);
                        return Some((Ok(answer), Some(kept.stamp)));
                    }
                }
            }
        }
        let stamp = Stamp { glance: mark.unwrap_or_default().to_string(), at: now };
        let read = (self.fetch)().and_then(|text| {
            // An answer that doesn't read is said in the reader's own words, as it was.
            let answer = (self.parse)(&text).map_err(|e| Failure::Other(e.to_string()))?;
            if mark.is_some() {
                save(dir, key, &stamp, &text);
            }
            Ok(answer)
        });
        Some(match read {
            Ok(answer) => (Ok(answer), Some(stamp)),
            Err(e) => (Err(e), None),
        })
    }
}

/// Everything a poll would bring, for an engine that starts with nothing and doesn't stay
/// (`--snapshot`): the kept answers when an engine that polls confirmed them all lately, which
/// asks GitHub nothing, else a poll.
pub fn read_once(slugs: &[String]) -> Vec<Answer> {
    let dir = saved_dir();
    if let Some(kept) = fresh(&dir, slugs, SystemTime::now()) {
        return kept;
    }
    let answers = std::sync::Mutex::new(Vec::new());
    let ask = Ask { slugs: slugs.to_vec(), ..Ask::default() };
    poll_in(&dir, &ask, &|answer| {
        if let Ok(mut all) = answers.lock() {
            all.push(answer);
        }
    });
    answers.into_inner().unwrap_or_default()
}

fn fresh(dir: &Path, slugs: &[String], now: SystemTime) -> Option<Vec<Answer>> {
    let lately = |key: &str| saved(dir, key).filter(|kept| now.duration_since(kept.stood).is_ok_and(|d| d < FRESH)).and_then(|kept| kept_answer(key, kept));
    let mut answers = vec![lately(REQUESTED)?];
    for slug in slugs {
        answers.push(lately(slug)?);
    }
    Some(answers)
}

/// A kept answer as a poll's.
fn kept_answer(key: &str, kept: Saved) -> Option<Answer> {
    Some(match key {
        REQUESTED => Answer::Requested(Ok(parse_requested(&kept.answer).ok()?), Some(kept.stamp)),
        slug => Answer::Repo(slug.to_string(), Ok(parse(&kept.answer).ok()?), Some(kept.stamp)),
    })
}

/// An answer kept on disk as GitHub gave it, and when it last stood: the file's time, which
/// `confirm` moves.
struct Saved {
    stamp: Stamp,
    answer: String,
    stood: SystemTime,
}

/// A cache: macOS may clear it, and a copy of the app with a home of its own has its own.
fn saved_dir() -> PathBuf {
    crate::util::home().join("Library/Caches/Codealong/github")
}

fn saved_path(dir: &Path, key: &str) -> PathBuf {
    let name: String = key.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '~' }).collect();
    dir.join(format!("{name}.json"))
}

fn saved(dir: &Path, key: &str) -> Option<Saved> {
    let path = saved_path(dir, key);
    let stood = std::fs::metadata(&path).ok()?.modified().ok()?;
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).ok()?).ok()?;
    let stamp = Stamp { glance: v.get("glance")?.as_str()?.to_string(), at: v.get("at")?.as_i64()? };
    Some(Saved { stamp, answer: v.get("answer")?.as_str()?.to_string(), stood })
}

/// Keeps an answer, in one step, readable only by you: it has private repos' PRs.
fn save(dir: &Path, key: &str, stamp: &Stamp, answer: &str) {
    let text = serde_json::json!({ "glance": stamp.glance, "at": stamp.at, "answer": answer }).to_string();
    let _ = crate::store::write_private(&saved_path(dir, key), text);
}

/// Says a kept answer still stands now.
fn confirm(dir: &Path, key: &str) {
    if let Ok(f) = std::fs::File::options().write(true).open(saved_path(dir, key)) {
        let _ = f.set_modified(SystemTime::now());
    }
}

// ---- One PR, for its detail panel ----

const DETAIL_QUERY: &str = r#"
query($owner: String!, $name: String!, $number: Int!) {
  viewer { login }
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      body bodyHTML mergeable
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
    /// The description as written, in Markdown: for a conversation about the PR to read
    /// (`for_reading`), not the windows, which show `body_html`.
    #[serde(skip)]
    pub body: String,
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
        body: pr.get("body").and_then(Value::as_str).unwrap_or_default().to_string(),
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

/// What a conversation about a PR reads of it (side.rs `ask_pr`), which has no `gh` of its own
/// to ask with: what it is, its description, reviews and comments as Markdown (`reading_md`),
/// and its whole diff as `gh pr diff` gives it. `detail`'s requests and the diff go in parallel.
/// A diff GitHub won't give whole (it refuses very large ones) is made of the files' own diffs,
/// which leave out the largest files, and the Markdown says so.
pub fn for_reading(pr: &crate::model::PrItem) -> Result<(String, String)> {
    let (detail, diff) = std::thread::scope(|scope| {
        let diff = scope.spawn(|| procs::run("gh", &["pr", "diff", &pr.number.to_string(), "-R", &pr.repo_slug], None, Duration::from_secs(60)));
        (detail(&pr.repo_slug, pr.number), diff.join())
    });
    let d = detail?;
    let (diff, whole) = match diff {
        Ok(Ok(diff)) => (diff, true),
        _ => (d.files.iter().filter_map(|f| f.patch.as_ref().map(|p| format!("diff --git a/{0} b/{0}\n{p}\n", f.path))).collect::<String>(), false),
    };
    Ok((reading_md(pr, &d, whole), diff))
}

/// A PR as Markdown, for a conversation about it to read (`for_reading`). `whole`: its diff is
/// all there, not only the files GitHub showed a diff for.
pub fn reading_md(pr: &crate::model::PrItem, d: &PrDetail, whole: bool) -> String {
    let mut md = format!("# #{} {}\n\n{} · by {} · {} into {} · {}\n", pr.number, pr.title, pr.repo_slug, pr.author.as_deref().unwrap_or("someone"), pr.head, pr.base.as_deref().unwrap_or("its base"), pr.url);
    md += &format!("\n## Description\n\n{}\n", if d.body.trim().is_empty() { "None." } else { d.body.trim() });
    if !d.reviews.is_empty() || !d.requested.is_empty() {
        md += "\n## Reviews\n\n";
        for r in &d.reviews {
            md += &format!("- {}: {}\n", r.author, r.state.to_lowercase().replace('_', " "));
        }
        for who in &d.requested {
            md += &format!("- {who}: review requested\n");
        }
    }
    if !d.comments.is_empty() {
        md += "\n## Comments, newest first\n";
        for c in &d.comments {
            let on = c.path.as_ref().map(|p| format!(" on `{p}`")).unwrap_or_default();
            let resolved = if c.resolved { " (resolved)" } else { "" };
            md += &format!("\n### {}{on}{resolved}\n\n{}\n", c.author, c.body.trim());
        }
    }
    md += &format!("\n## Files changed ({})\n\n", pr.changed_files);
    for f in &d.files {
        md += &format!("- {} `{}` +{} −{}\n", f.status, f.path, f.additions, f.deletions);
    }
    if d.more_files {
        md += "- …and more, which GitHub lists past the first hundred.\n";
    }
    if !whole {
        md += "\nGitHub wouldn't give the whole diff, so pr.diff has only the files above that GitHub showed a diff for.\n";
    }
    md
}

/// What a review of a PR says: approved, a comment, or changes asked for. A comment or a request
/// for changes needs something written; an approval's text is optional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    Approve,
    Comment,
    RequestChanges,
}

impl Verdict {
    /// `gh pr review`'s flag for it.
    fn flag(self) -> &'static str {
        match self {
            Verdict::Approve => "--approve",
            Verdict::Comment => "--comment",
            Verdict::RequestChanges => "--request-changes",
        }
    }

    /// What was done, for a message: "approve" in "Couldn't approve #12".
    pub fn verb(self) -> &'static str {
        match self {
            Verdict::Approve => "approve",
            Verdict::Comment => "comment on",
            Verdict::RequestChanges => "request changes on",
        }
    }
}

/// The arguments `gh pr review` is given for a verdict with its text, as `review` runs it.
pub fn review_args(number: u64, verdict: Verdict, body: &str, slug: &str) -> Result<Vec<String>> {
    let body = body.trim();
    if body.is_empty() && verdict != Verdict::Approve {
        return Err(anyhow!("Write what to say first: a {} needs text.", if verdict == Verdict::Comment { "comment" } else { "request for changes" }));
    }
    let mut args: Vec<String> = vec!["pr".into(), "review".into(), number.to_string(), verdict.flag().into(), "-R".into(), slug.into()];
    if !body.is_empty() {
        args.push("--body".into());
        args.push(body.into());
    }
    Ok(args)
}

/// Reviews a PR as you, through `gh`: approves it, comments on it or asks for changes, with what
/// you wrote as the review's text.
pub fn review(slug: &str, number: u64, verdict: Verdict, body: &str) -> Result<()> {
    let args = review_args(number, verdict, body, slug)?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    procs::run("gh", &args, None, Duration::from_secs(30))
        .map(|_| ())
        .map_err(|e| anyhow!("Couldn't {} #{number}: {e}", verdict.verb()))
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
    fn reads_a_repos_name_now() {
        assert_eq!(slug_of("sprucehq/spark\n").as_deref(), Some("sprucehq/spark"));
        assert_eq!(slug_of("Spur-HQ/spur.web").as_deref(), Some("Spur-HQ/spur.web"));
        for bad in ["", "null", "sprucehq", "a/b/c", "gh: Not Found (HTTP 404)"] {
            assert_eq!(slug_of(bad), None, "{bad}");
        }
    }

    /// A repo's PRs as GitHub answers: an open one whose checks are running, and a merged one.
    const PRS: &str = r#"{"data":{"viewer":{"login":"gwen"},"requested":{"nodes":[{"number":1037}]},"repository":{
          "open":{"nodes":[{"number":1037,"title":"Let the agent add and caption project photos","url":"https://github.com/sprucehq/spark/pull/1037","state":"OPEN","isDraft":false,"reviewDecision":"APPROVED","mergeStateStatus":"UNSTABLE","updatedAt":"2026-09-23T20:00:00Z","mergedAt":null,"closedAt":null,"headRefName":"portal","headRefOid":"0ef4ed2","headRepositoryOwner":{"login":"gwen"},
             "commits":{"nodes":[{"commit":{"statusCheckRollup":{"contexts":{"nodes":[
               {"__typename":"CheckRun","name":"server:test","status":"COMPLETED","conclusion":"SUCCESS"},
               {"__typename":"CheckRun","name":"docker-smoke","status":"COMPLETED","conclusion":"FAILURE"},
               {"__typename":"CheckRun","name":"lint","status":"IN_PROGRESS","conclusion":null},
               {"__typename":"CheckRun","name":"deploy","status":"COMPLETED","conclusion":"SKIPPED"},
               {"__typename":"StatusContext","context":"vercel","state":"SUCCESS"}]}}}}]}}]},
          "done":{"nodes":[{"number":1016,"title":"Prevent page shifts","url":"u","state":"MERGED","isDraft":false,"reviewDecision":"APPROVED","updatedAt":"2026-09-23T18:00:00Z","mergedAt":"2026-09-23T18:00:00Z","closedAt":"2026-09-23T18:00:00Z","headRefName":"app6","headRepositoryOwner":{"login":"gwen"},"commits":{"nodes":[]}}]}}}}"#;

    const GLANCE: &str = r#"{"data":{"viewer":{"login":"gwen"},
      "r0":{"open":{"nodes":[{"number":1037,"updatedAt":"2026-09-23T20:00:00Z","headRefOid":"0ef4ed2","isDraft":false,"reviewDecision":"APPROVED","mergeStateStatus":"CLEAN","statusCheckRollup":{"state":"SUCCESS"}}]},"done":{"nodes":[{"number":1016,"updatedAt":"2026-09-23T18:00:00Z"}]}},
      "r1":{"open":{"nodes":[]},"done":{"nodes":[]}},
      "r2":null,
      "search":{"nodes":[{"number":7,"updatedAt":"2026-09-23T19:00:00Z","repository":{"nameWithOwner":"Spurhq/Spur"}}]}},
      "errors":[{"type":"NOT_FOUND","path":["r2"],"message":"Could not resolve to a Repository with the name 'gone/gone'."}]}"#;

    fn slugs() -> Vec<String> {
        vec!["sprucehq/spark".into(), "spurhq/spur".into(), "gone/gone".into()]
    }

    #[test]
    fn a_glance_marks_each_repo_and_changes_with_its_prs() {
        let seen = parse_glance(GLANCE, &slugs()).unwrap();
        assert_eq!(seen.login.as_deref(), Some("gwen"));
        assert_eq!(seen.repos["gone/gone"], None, "a repo GitHub doesn't show you");
        let mark = |g: &Glance, slug: &str| g.repos[slug].clone().unwrap();
        assert_eq!(parse_glance(GLANCE, &slugs()).unwrap(), seen, "the same answer, the same marks");

        // A PR's checks finish: only its repo's mark moves.
        let checked = parse_glance(&GLANCE.replace(r#"{"state":"SUCCESS"}"#, r#"{"state":"FAILURE"}"#), &slugs()).unwrap();
        assert_ne!(mark(&checked, "sprucehq/spark"), mark(&seen, "sprucehq/spark"));
        assert_eq!(mark(&checked, "spurhq/spur"), mark(&seen, "spurhq/spur"));
        assert_eq!(checked.requested, seen.requested);

        // A closed PR changes: it's the latest updated one.
        let closed = parse_glance(&GLANCE.replace("2026-09-23T18:00:00Z", "2026-09-23T18:30:00Z"), &slugs()).unwrap();
        assert_ne!(mark(&closed, "sprucehq/spark"), mark(&seen, "sprucehq/spark"));

        // The search lists another PR of a repo, as it does when a team you're on is asked:
        // the search's mark moves, and that repo's, whatever case its name is in.
        let asked = parse_glance(&GLANCE.replace(r#""number":7,"#, r#""number":8,"#), &slugs()).unwrap();
        assert_ne!(asked.requested, seen.requested);
        assert_ne!(mark(&asked, "spurhq/spur"), mark(&seen, "spurhq/spur"));
        assert_eq!(mark(&asked, "sprucehq/spark"), mark(&seen, "sprucehq/spark"));

        // Another sign-in sees other requests.
        let other = parse_glance(&GLANCE.replace("gwen", "someone"), &slugs()).unwrap();
        assert_ne!(mark(&other, "spurhq/spur"), mark(&seen, "spurhq/spur"));
    }

    #[test]
    fn a_glance_needs_github_to_have_answered() {
        // Out of points: errors and no data.
        assert!(parse_glance(r#"{"errors":[{"type":"RATE_LIMITED","message":"API rate limit exceeded for user ID 1."}]}"#, &slugs()).is_err());
        assert!(parse_glance("", &slugs()).is_err());
        let said = failed(&anyhow!("gh gh: API rate limit exceeded for user ID 1."), "PRs");
        assert!(said.rate_limited(), "{said}");
        assert!(failed(&anyhow!("gh gh: You have exceeded a secondary rate limit."), "PRs").rate_limited());
        assert!(!failed(&anyhow!("gh timed out after 30s"), "PRs").rate_limited());
        assert_eq!(said.title(), "GitHub rate limit reached");
        assert_eq!(failed(&anyhow!("couldn't start gh: No such file"), "PRs").title(), "GitHub CLI isn't installed");
        assert_eq!(failed(&anyhow!("gh HTTP 401: Bad credentials"), "PRs").title(), "Signed out of GitHub");
        assert_eq!(failed(&anyhow!("gh timed out after 30s"), "PRs").title(), "GitHub didn't answer");
    }

    /// The board says each failure in the words it always has: a window knows a notice it has
    /// dismissed by them.
    #[test]
    fn a_failure_is_said_in_the_same_words() {
        let said = |message: &str| failed(&anyhow!("{message}"), "PRs for a/b");
        assert_eq!(said("couldn't start gh: No such file"), Failure::NotInstalled);
        assert_eq!(said("couldn't start gh: No such file").to_string(), "GitHub: install the gh CLI and sign in with `gh auth login` to see PRs");
        assert_eq!(said("gh To get started with GitHub CLI, please run:  gh auth login"), Failure::SignedOut);
        assert_eq!(said("gh HTTP 401: Bad credentials").to_string(), "GitHub: sign in again with `gh auth login` to see PRs");
        assert_eq!(said("gh gh: API rate limit exceeded for user ID 1."), Failure::RateLimited { resets: None });
        assert_eq!(
            Failure::RateLimited { resets: Some(1) }.to_string(),
            "GitHub: your account's rate limit is used up, so PRs wait until it resets. `gh` and your agents draw on the same limit."
        );
        assert_eq!(said("gh timed out after 30s"), Failure::Other("GitHub: couldn't load PRs for a/b".into()));
        assert_eq!(said("gh timed out after 30s").to_string(), "GitHub: couldn't load PRs for a/b");
        // Sign-in is looked at before the rate limit, as it was.
        assert_eq!(said("gh HTTP 401: rate limit"), Failure::SignedOut);
    }

    #[test]
    fn a_glance_says_what_it_saw_of_a_repo() {
        let seen = parse_glance(GLANCE, &slugs()).unwrap();
        assert!(matches!(seen.seen("sprucehq/spark"), Seen::Mark(mark) if Some(mark) == seen.repos["sprucehq/spark"].as_deref()));
        assert!(matches!(seen.seen("gone/gone"), Seen::Hidden));
        assert!(matches!(seen.seen("never/asked"), Seen::Unknown), "a repo whose query failed isn't listed");
    }

    #[test]
    fn an_answers_headers_say_what_points_are_left() {
        let out = "HTTP/2.0 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nX-Ratelimit-Remaining: 4944\r\nX-Ratelimit-Reset: 1790898270\r\nX-Ratelimit-Resource: graphql\r\n\r\n{\"data\":{}}";
        let (points, body) = answer_parts(out);
        assert_eq!(points, Some(Points { left: 4944, resets: 1_790_898_270_000 }));
        assert_eq!(body, "{\"data\":{}}");
        // A refusal has them too, and says when the hour ends.
        let (points, body) = answer_parts("HTTP/2.0 200 OK\nx-ratelimit-remaining: 0\nx-ratelimit-reset: 1790898270\n\n{\"errors\":[]}");
        assert_eq!((points.map(|p| p.left), body), (Some(0), "{\"errors\":[]}"));
        // Without headers (gh couldn't reach GitHub, or printed only an error), all of it is the body.
        assert_eq!(answer_parts("{\"data\":{}}"), (None, "{\"data\":{}}"));
        assert_eq!(answer_parts(""), (None, ""));
        assert_eq!(answer_parts("HTTP/2.0 502 Bad Gateway\r\n\r\n").0, None);
    }

    #[test]
    fn a_glance_query_names_each_repo() {
        let none = glance_query(0);
        assert!(!none.contains("fragment") && none.contains("search(") && none.contains("query($requested: String!)"), "{none}");
        let two = glance_query(2);
        assert!(two.contains("$o1: String!, $n1: String!") && two.contains("r1: repository(owner: $o1, name: $n1) { ...glance }") && two.contains("fragment glance"), "{two}");
    }

    #[test]
    fn an_answer_stands_while_its_glance_does() {
        let now = 1_000_000_000;
        let stamp = Stamp { glance: "a".into(), at: now - 60_000 };
        assert!(stands(&stamp, "a", false, false, now));
        assert!(!stands(&stamp, "b", false, false, now), "the repo's PRs changed");
        assert!(!stands(&stamp, "a", true, false, now), "its checks are running");
        assert!(stands(&Stamp { at: now - 20_000, ..stamp.clone() }, "a", true, false, now), "but it was just read");
        let old = Stamp { glance: "a".into(), at: now - RESYNC_MS };
        assert!(!stands(&old, "a", false, false, now), "read again every so often");
        assert!(stands(&old, "a", true, true, now), "unless points are short");
        assert!(!stands(&old, "b", true, true, now));
        // Read when GitHub gave no glance.
        assert!(!stands(&Stamp { glance: String::new(), at: now }, "a", false, false, now));
    }

    #[test]
    fn checks_keep_a_repo_busy_for_a_while() {
        let now = 1_000_000_000_000;
        let prs = parse(PRS).unwrap().prs;
        let pending = |updated_at: i64, state: PrState| vec![PrRaw { checks: Checks { pending: 1, ..Checks::default() }, updated_at, state, ..prs[0].clone() }];
        assert!(busy(&pending(now - 60_000, PrState::Open), now));
        assert!(!busy(&pending(now - BUSY_FOR_MS, PrState::Open), now), "pending since long ago: stuck, or waiting on someone");
        assert!(!busy(&pending(now - 60_000, PrState::Merged), now));
        assert!(!busy(&[PrRaw { checks: Checks::default(), updated_at: now, ..prs[0].clone() }], now));
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("codealong-github-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// `read` for a repo, with GitHub answering `PRS` and counting how often it's asked.
    fn read_repo(dir: &Path, ask: &Ask, mark: Option<&str>, now: i64, asked: &std::cell::Cell<u32>) -> Option<Brought<RepoPrs>> {
        let fetch = || -> std::result::Result<String, Failure> {
            asked.set(asked.get() + 1);
            Ok(PRS.to_string())
        };
        Reading { dir, ask, key: "sprucehq/spark", mark, now, fetch: &fetch, parse: &parse, busy: &|repo: &RepoPrs| busy(&repo.prs, now) }.read()
    }

    #[test]
    fn a_poll_reads_a_repo_only_when_its_glance_changed() {
        let dir = scratch("read");
        let asked = std::cell::Cell::new(0);
        // Long after the PR in `PRS` last changed: its pending check keeps nothing busy.
        let now = 1_795_000_000_000;
        let slug = "sprucehq/spark";
        let mut ask = Ask { slugs: vec![slug.into()], ..Ask::default() };

        // Nothing had, nothing kept: GitHub is asked, and the answer kept with its glance.
        let (result, stamp) = read_repo(&dir, &ask, Some("a"), now, &asked).unwrap();
        assert_eq!((result.unwrap().prs.len(), asked.get()), (2, 1));
        assert_eq!(stamp, Some(Stamp { glance: "a".into(), at: now }));
        ask.have.insert(slug.into(), stamp.unwrap());

        // The same glance a minute later: what's had stands.
        assert!(read_repo(&dir, &ask, Some("a"), now + 60_000, &asked).is_none());
        // Another engine starts with nothing: the kept answer is its answer.
        let fresh = Ask { slugs: vec![slug.into()], ..Ask::default() };
        let (result, stamp) = read_repo(&dir, &fresh, Some("a"), now + 60_000, &asked).unwrap();
        assert_eq!((result.unwrap().prs.len(), stamp.unwrap().at, asked.get()), (2, now, 1));

        // The kept answer has checks running as of `then`: kept a minute, it's read again.
        let then = parse(PRS).unwrap().prs[0].updated_at + 120_000;
        save(&dir, slug, &Stamp { glance: "a".into(), at: then }, PRS);
        assert!(read_repo(&dir, &fresh, Some("a"), then + 20_000, &asked).is_some());
        assert_eq!(asked.get(), 1);
        assert!(read_repo(&dir, &fresh, Some("a"), then + 60_000, &asked).is_some());
        assert_eq!(asked.get(), 2);

        // The glance changed: asked again.
        let (_, stamp) = read_repo(&dir, &ask, Some("b"), now + 120_000, &asked).unwrap();
        assert_eq!((stamp.clone().unwrap().glance.as_str(), asked.get()), ("b", 3));
        ask.have.insert(slug.into(), stamp.unwrap());
        // Its checks are running: asked at each poll.
        ask.busy.insert(slug.into());
        assert!(read_repo(&dir, &ask, Some("b"), now + 180_000, &asked).is_some());
        assert_eq!(asked.get(), 4);
        ask.busy.clear();
        // Read again once it's old, and at once after something you did there.
        assert!(read_repo(&dir, &ask, Some("b"), now + 180_000 + RESYNC_MS, &asked).is_some());
        ask.all = true;
        assert!(read_repo(&dir, &ask, Some("b"), now + 180_000, &asked).is_some());
        assert_eq!(asked.get(), 6);
        ask.all = false;

        // GitHub gave no glance: what's had stands for a while, and an answer read then isn't
        // kept, nor taken for one read at a glance.
        ask.have.insert(slug.into(), Stamp { glance: "b".into(), at: now });
        assert!(read_repo(&dir, &ask, None, now + 60_000, &asked).is_none());
        let (_, stamp) = read_repo(&dir, &ask, None, now + RESYNC_MS, &asked).unwrap();
        assert_eq!((stamp, asked.get()), (Some(Stamp { glance: String::new(), at: now + RESYNC_MS }), 7));
        assert_eq!(saved(&dir, slug).unwrap().stamp.glance, "b");

        // A failure has no stamp, so the next poll asks again.
        let refused = || -> std::result::Result<String, Failure> { Err(Failure::Other("GitHub: couldn't load PRs for a/b".into())) };
        let failing = Reading::<RepoPrs> { dir: &dir, ask: &fresh, key: "a/b", mark: Some("a"), now, fetch: &refused, parse: &parse, busy: &|_| false }.read().unwrap();
        assert_eq!((failing.0.unwrap_err().to_string().as_str(), failing.1), ("GitHub: couldn't load PRs for a/b", None));
        assert!(saved(&dir, "a/b").is_none());
        // An answer that doesn't read is a failure in the reader's words, and isn't kept.
        let garbled = Reading::<RepoPrs> { dir: &dir, ask: &fresh, key: "a/b", mark: Some("a"), now, fetch: &|| Ok("{}".into()), parse: &parse, busy: &|_| false }.read().unwrap();
        assert_eq!(garbled.0.unwrap_err(), Failure::Other("no repository in response".into()));
        assert!(saved(&dir, "a/b").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_snapshot_takes_answers_confirmed_lately() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("fresh");
        let slugs = vec!["sprucehq/spark".to_string()];
        let stamp = Stamp { glance: "a".into(), at: 1 };
        let search = r#"{"data":{"viewer":{"login":"gwen"},"search":{"nodes":[]}}}"#;
        assert!(fresh(&dir, &slugs, SystemTime::now()).is_none(), "nothing kept");
        save(&dir, REQUESTED, &stamp, search);
        assert!(fresh(&dir, &slugs, SystemTime::now()).is_none(), "a repo's answer is missing");
        save(&dir, &slugs[0], &stamp, PRS);
        assert_eq!(std::fs::metadata(saved_path(&dir, &slugs[0])).unwrap().permissions().mode() & 0o777, 0o600);
        let answers = fresh(&dir, &slugs, SystemTime::now()).unwrap();
        assert!(matches!(&answers[0], Answer::Requested(Ok(found), _) if found.prs.is_empty()));
        assert!(matches!(&answers[1], Answer::Repo(slug, Ok(repo), Some(at)) if slug == &slugs[0] && repo.prs.len() == 2 && at == &stamp));

        // Nobody has confirmed them for a while: GitHub is asked.
        let later = SystemTime::now() + FRESH;
        assert!(fresh(&dir, &slugs, later).is_none());
        // An engine that polls says they still stand.
        let old = SystemTime::now() - Duration::from_secs(3600);
        for key in [REQUESTED, slugs[0].as_str()] {
            std::fs::File::options().write(true).open(saved_path(&dir, key)).unwrap().set_modified(old).unwrap();
        }
        assert!(fresh(&dir, &slugs, SystemTime::now()).is_none());
        confirm(&dir, REQUESTED);
        confirm(&dir, &slugs[0]);
        assert!(fresh(&dir, &slugs, SystemTime::now()).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn kept_answers_are_files_named_for_their_repos() {
        let dir = Path::new("/c");
        assert_eq!(saved_path(dir, "Sprucehq/spa.rk"), Path::new("/c/sprucehq~spa.rk.json"));
        assert_eq!(saved_path(dir, REQUESTED), Path::new("/c/requested.json"));
        assert_eq!(saved_path(dir, "../x"), Path::new("/c/..~x.json"));
    }

    #[test]
    fn parses_prs_and_checks() {
        let repo = parse(PRS).unwrap();
        let prs = repo.prs;
        assert_eq!(prs.len(), 2);
        assert_eq!(prs[0].state, PrState::Approved);
        assert_eq!(prs[0].head_owner.as_deref(), Some("gwen"));
        assert_eq!(prs[0].head_oid.as_deref(), Some("0ef4ed2"));
        assert_eq!((prs[0].checks.passed, prs[0].checks.failed, prs[0].checks.pending, prs[0].checks.skipped), (2, 1, 1, 1));
        assert_eq!(prs[0].checks.failing, vec!["docker-smoke".to_string()]);
        assert_eq!(prs[0].merge, Merge::Unstable);
        assert_eq!(prs[1].state, PrState::Merged);
        assert_eq!(prs[1].merge, Merge::Unknown);
        assert!(prs[1].merged_at.is_some());
        assert_eq!(repo.login.as_deref(), Some("gwen"));
        assert!(prs[0].review_requested);
        assert!(!prs[1].review_requested);
    }

    #[test]
    fn reads_a_request_to_you_from_the_pr() {
        // #1 you've just approved (the search hasn't caught up), #2 was just asked of you (nor
        // has it), #3 waits on a team you're on, #4 on someone else.
        let open = [(1, r#"[{"requestedReviewer":{"__typename":"User","login":"whather"}}]"#), (2, r#"[{"requestedReviewer":{"__typename":"User","login":"GWen"}}]"#), (3, r#"[{"requestedReviewer":{"__typename":"Team"}}]"#), (4, r#"[{"requestedReviewer":{"__typename":"User","login":"whather"}}]"#)]
            .map(|(n, asked)| format!(r#"{{"number":{n},"title":"t","url":"u","state":"OPEN","isDraft":false,"reviewDecision":null,"updatedAt":"2026-09-30T20:00:00Z","headRefName":"b{n}","commits":{{"nodes":[]}},"reviewRequests":{{"nodes":{asked}}}}}"#))
            .join(",");
        let json = format!(r#"{{"data":{{"viewer":{{"login":"gwen"}},"requested":{{"nodes":[{{"number":1}},{{"number":3}}]}},"repository":{{"open":{{"nodes":[{open}]}},"done":{{"nodes":[]}}}}}}}}"#);
        let asked: Vec<_> = parse(&json).unwrap().prs.iter().map(|p| (p.number, p.review_requested)).collect();
        assert_eq!(asked, vec![(1, false), (2, true), (3, true), (4, false)]);
    }

    #[test]
    fn reads_who_asked_for_your_review() {
        let event = |actor: &str, reviewer: &str| format!(r#"{{"actor":{{"login":"{actor}"}},"requestedReviewer":{reviewer}}}"#);
        let (me, other, team) = (r#"{"__typename":"User","login":"GWen"}"#, r#"{"__typename":"User","login":"ana"}"#, r#"{"__typename":"Team"}"#);
        let asker = |events: &[String]| {
            let n: Value = serde_json::from_str(&format!(r#"{{"timelineItems":{{"nodes":[{}]}}}}"#, events.join(","))).unwrap();
            asker_of(&n, Some("gwen"))
        };
        // The latest request to you, whoever else was asked since, and not the author's own.
        assert_eq!(asker(&[event("bot", me), event("ken", me), event("lee", other), event("lee", team)]).as_deref(), Some("ken"));
        // Through a team, or a reviewer you can't see, when none names you.
        assert_eq!(asker(&[event("ken", other), event("lee", team), event("ana", other)]).as_deref(), Some("lee"));
        assert_eq!(asker(&[event("ken", "null")]).as_deref(), Some("ken"));
        assert_eq!(asker(&[event("ken", other)]), None);
        assert_eq!(asker(&[]), None);
        assert_eq!(asker_of(&serde_json::json!({}), Some("gwen")), None);
    }

    #[test]
    fn reads_when_your_review_was_asked_for() {
        let event = |at: &str, reviewer: &str| format!(r#"{{"createdAt":"{at}","actor":{{"login":"ken"}},"requestedReviewer":{reviewer}}}"#);
        let (me, other) = (r#"{"__typename":"User","login":"gwen"}"#, r#"{"__typename":"User","login":"ana"}"#);
        let at = |events: &[String]| {
            let n: Value = serde_json::from_str(&format!(r#"{{"timelineItems":{{"nodes":[{}]}}}}"#, events.join(","))).unwrap();
            asked_at(&n, Some("gwen"))
        };
        // The latest request to you, not a later one to someone else.
        assert_eq!(at(&[event("2026-09-30T20:00:00Z", me), event("2026-10-01T08:00:00Z", me), event("2026-10-02T09:00:00Z", other)]), parse_rfc3339_ms("2026-10-01T08:00:00Z"));
        assert_eq!(at(&[event("2026-09-30T20:00:00Z", other)]), None);
        assert_eq!(at(&[]), None);
    }

    #[test]
    fn a_review_is_run_through_gh_with_its_text() {
        let args = |verdict: Verdict, body: &str| review_args(12, verdict, body, "acme/site").map(|a| a.join(" "));
        assert_eq!(args(Verdict::Approve, "").unwrap(), "pr review 12 --approve -R acme/site");
        assert_eq!(args(Verdict::Approve, " Nice. ").unwrap(), "pr review 12 --approve -R acme/site --body Nice.");
        assert_eq!(args(Verdict::Comment, "A thought").unwrap(), "pr review 12 --comment -R acme/site --body A thought");
        assert_eq!(args(Verdict::RequestChanges, "Fix it").unwrap(), "pr review 12 --request-changes -R acme/site --body Fix it");
        // A comment or a request for changes says something or isn't sent.
        assert!(args(Verdict::Comment, "  ").is_err());
        assert!(args(Verdict::RequestChanges, "").is_err());
        assert_eq!(serde_json::to_string(&Verdict::RequestChanges).unwrap(), "\"request-changes\"");
    }

    #[test]
    fn reads_the_prs_that_wait_on_your_review_anywhere() {
        // #1 in one repo and #7 in another wait on you, #2 you've reviewed (the search hasn't
        // caught up), #3 waits on a team you're on; the last node isn't a PR.
        let pr = |slug: &str, n: u64, asked: &str| {
            format!(r#"{{"number":{n},"title":"t","url":"u","state":"OPEN","isDraft":false,"reviewDecision":null,"updatedAt":"2026-09-30T20:00:00Z","headRefName":"b{n}","commits":{{"nodes":[]}},"reviewRequests":{{"nodes":{asked}}},"repository":{{"nameWithOwner":"{slug}"}}}}"#)
        };
        let me = r#"[{"requestedReviewer":{"__typename":"User","login":"gwen"}}]"#;
        let nodes = [pr("acme/site", 1, me), pr("acme/site", 2, "[]"), pr("acme/tool", 3, r#"[{"requestedReviewer":{"__typename":"Team"}}]"#), pr("acme/tool", 7, me), "{}".to_string()].join(",");
        let json = format!(r#"{{"data":{{"viewer":{{"login":"gwen"}},"search":{{"nodes":[{nodes}]}}}}}}"#);
        let found = parse_requested(&json).unwrap();
        let got: Vec<(&str, u64)> = found.prs.iter().map(|(slug, p)| (slug.as_str(), p.number)).collect();
        assert_eq!(got, vec![("acme/site", 1), ("acme/tool", 3), ("acme/tool", 7)]);
        assert!(found.prs.iter().all(|(_, p)| p.review_requested));
        assert_eq!(found.login.as_deref(), Some("gwen"));
    }

    #[test]
    fn reads_one_pr() {
        let json = r#"{"data":{"viewer":{"login":"me"},"repository":{"pullRequest":{"number":88,"title":"Old work","url":"https://github.com/acme/tool/pull/88","state":"MERGED","isDraft":false,"reviewDecision":null,"mergeStateStatus":"CLEAN","updatedAt":"2026-08-01T10:00:00Z","createdAt":"2026-07-30T10:00:00Z","mergedAt":"2026-08-01T10:00:00Z","closedAt":"2026-08-01T10:00:00Z","headRefName":"old-work","baseRefName":"main","additions":12,"deletions":3,"changedFiles":2,"author":{"login":"me"},"commits":{"nodes":[]}}}}}"#;
        let (pr, login) = parse_one(json).unwrap();
        assert_eq!((pr.number, pr.state, pr.merge), (88, PrState::Merged, Merge::Unknown));
        assert_eq!((pr.head_ref.as_str(), pr.base_ref.as_deref(), pr.changed_files), ("old-work", Some("main"), 2));
        assert_eq!(login.as_deref(), Some("me"));
        assert!(parse_one(r#"{"data":{"viewer":{"login":"me"},"repository":{"pullRequest":null}}}"#).is_err());
    }

    #[test]
    fn keeps_each_reviewers_standing_review() {
        let graph = r#"{"data":{"repository":{"pullRequest":{"bodyHTML":"<p>Hi</p>","mergeable":"MERGEABLE","commits":{"totalCount":3},
          "reviews":{"nodes":[
            {"author":{"login":"ken"},"state":"APPROVED","submittedAt":"2026-09-23T10:00:00Z"},
            {"author":{"login":"ken"},"state":"COMMENTED","submittedAt":"2026-09-23T11:00:00Z"},
            {"author":{"login":"ana"},"state":"COMMENTED","submittedAt":"2026-09-23T12:00:00Z"},
            {"author":{"login":"ana"},"state":"CHANGES_REQUESTED","submittedAt":"2026-09-23T13:00:00Z"}]},
          "reviewRequests":{"nodes":[{"requestedReviewer":{"__typename":"User","login":"gwen"}},{"requestedReviewer":{"__typename":"Team","name":"platform"}}]}}}}}"#;
        let files = r#"[{"filename":"src/a.ts","status":"modified","additions":3,"deletions":1,"patch":"@@ -1 +1 @@"},{"filename":"big.lock","status":"modified","additions":900,"deletions":800}]"#;
        let d = parse_detail(graph, files).unwrap();
        let states: Vec<(&str, &str)> = d.reviews.iter().map(|r| (r.author.as_str(), r.state.as_str())).collect();
        assert_eq!(states, vec![("ken", "APPROVED"), ("ana", "CHANGES_REQUESTED")]);
        assert_eq!(d.requested, vec!["gwen".to_string(), "platform".to_string()]);
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

    /// What a conversation about a PR reads of it: the description as written, where reviews
    /// stand, the comments with the files they're on, and the files.
    #[test]
    fn a_pr_is_written_out_for_a_conversation_to_read() {
        let graph = r#"{"data":{"viewer":{"login":"me"},"repository":{"pullRequest":{"body":"Adds **teams**.","bodyHTML":"<p>Adds <b>teams</b>.</p>","mergeable":null,"commits":{"totalCount":1},
          "reviews":{"nodes":[{"id":"R1","author":{"login":"ana"},"state":"CHANGES_REQUESTED","submittedAt":"2026-09-23T13:00:00Z","body":"Please add a test.","url":"u1"}]},
          "reviewRequests":{"nodes":[{"requestedReviewer":{"__typename":"User","login":"gwen"}}]},
          "comments":{"nodes":[]},
          "reviewThreads":{"nodes":[{"isResolved":true,"path":"src/a.ts","comments":{"nodes":[{"id":"T1","author":{"login":"ana"},"body":"Why this?","createdAt":"2026-09-23T12:00:00Z","url":"u4"}]}}]}}}}}"#;
        let files = r#"[{"filename":"src/a.ts","status":"modified","additions":3,"deletions":1,"patch":"@@ -1 +1 @@"}]"#;
        let d = parse_detail(graph, files).unwrap();
        assert_eq!(d.body, "Adds **teams**.");
        let pr: crate::model::PrItem = serde_json::from_value(serde_json::json!({
            "key": "o/r#7", "repoSlug": "o/r", "repo": "r", "project": "P", "number": 7, "title": "Add teams", "url": "https://github.com/o/r/pull/7",
            "state": "open", "author": "ken", "mine": false, "reviewRequested": true, "head": "feat", "base": "main",
            "checks": {"passed": 0, "failed": 0, "pending": 0, "skipped": 0, "failing": []}, "merge": "unknown",
            "additions": 3, "deletions": 1, "changedFiles": 1, "createdAt": 0, "updatedAt": 0, "mergedAt": null, "closedAt": null, "lanes": [], "sessions": []
        }))
        .unwrap();
        let md = reading_md(&pr, &d, true);
        assert_eq!(
            md,
            "# #7 Add teams\n\no/r · by ken · feat into main · https://github.com/o/r/pull/7\n\n## Description\n\nAdds **teams**.\n\n## Reviews\n\n- ana: changes requested\n- gwen: review requested\n\n## Comments, newest first\n\n### ana\n\nPlease add a test.\n\n### ana on `src/a.ts` (resolved)\n\nWhy this?\n\n## Files changed (1)\n\n- modified `src/a.ts` +3 −1\n"
        );
        assert!(reading_md(&pr, &d, false).ends_with("pr.diff has only the files above that GitHub showed a diff for.\n"));
    }
}
