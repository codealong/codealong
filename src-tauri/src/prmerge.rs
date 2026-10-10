//! Merging a PR as GitHub's merge box does, through `gh api graphql` as you. `merge_box` reads
//! what the box offers: the methods the repo allows and the one GitHub picks for you, the title
//! and message GitHub would write for a merge commit and for a squash (the repo's own settings
//! for them already applied), your commit emails, auto-merge, the merge queue, merging past the
//! rules for one who may, bringing the branch up to date, taking it out of draft, and deleting
//! the branch. `act` does one of them. What merges or moves the branch names the head commit the
//! box was read at, so GitHub refuses it when the branch has moved since: you merge what you saw.
//!
//! These are the mutations `gh pr merge` sends itself (`--admin` is a plain merge, which GitHub
//! lets through for whoever may bypass the rules; `--auto` enables auto-merge; a base branch
//! with a merge queue gets an enqueue), asked directly so the box can offer each of them.

use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::procs;
use crate::util::parse_rfc3339_ms;

/// What the box reads. `viewerCanDeleteHeadRef` isn't asked: with `mergeStateStatus` and
/// `viewerCanUpdateBranch` beside it, GitHub fails the whole query for a PR from a fork ("Something
/// went wrong while executing your query", every time, even with it under an alias of its own;
/// 2026-10-09). Whether the branch can go is told from the push access its repo gives you.
const QUERY: &str = r#"
query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    viewerPermission mergeCommitAllowed squashMergeAllowed rebaseMergeAllowed deleteBranchOnMerge
    viewerDefaultMergeMethod viewerDefaultCommitEmail viewerPossibleCommitEmails
    pullRequest(number: $number) {
      id state isDraft mergeable mergeStateStatus reviewDecision headRefOid headRefName
      headRef { id } headRepository { viewerPermission }
      viewerCanUpdate viewerCanMergeAsAdmin viewerCanEnableAutoMerge viewerCanDisableAutoMerge viewerCanUpdateBranch
      isMergeQueueEnabled isInMergeQueue mergeQueueEntry { position state }
      autoMergeRequest { mergeMethod enabledAt enabledBy { login } }
      mergeTitle: viewerMergeHeadlineText(mergeType: MERGE) mergeBody: viewerMergeBodyText(mergeType: MERGE)
      squashTitle: viewerMergeHeadlineText(mergeType: SQUASH) squashBody: viewerMergeBodyText(mergeType: SQUASH)
      mergedBy { login }
    }
  }
}
"#;

/// How a PR's commits go onto its base: GitHub's three ways.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Method {
    /// A merge commit.
    Merge,
    Squash,
    Rebase,
}

impl Method {
    const ALL: [Method; 3] = [Method::Merge, Method::Squash, Method::Rebase];

    fn graphql(self) -> &'static str {
        match self {
            Method::Merge => "MERGE",
            Method::Squash => "SQUASH",
            Method::Rebase => "REBASE",
        }
    }

    fn of(s: Option<&str>) -> Option<Method> {
        Method::ALL.into_iter().find(|m| Some(m.graphql()) == s)
    }

    /// Whether the commit it makes takes a title and a message: a rebase makes none of its own.
    fn writes(self) -> bool {
        self != Method::Rebase
    }
}

/// A commit's title and message, as GitHub would write them.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitText {
    pub title: String,
    pub body: String,
}

/// Auto-merge, while it's on: who turned it on and when, and how it'll merge.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoMerge {
    pub method: Option<Method>,
    pub by: Option<String>,
    pub at: Option<i64>,
}

/// Its place in the merge queue.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Queued {
    pub position: Option<u64>,
    /// `QUEUED`, `AWAITING_CHECKS`, `MERGEABLE`, `UNMERGEABLE` or `LOCKED`.
    pub state: Option<String>,
}

/// What a PR's merge box offers you, and where the PR stands for it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeBox {
    /// The PR's node id, which every act names it by.
    pub id: String,
    /// The head commit the box was read at, which a merge or an update names.
    pub head: String,
    pub branch: String,
    /// `OPEN`, `MERGED` or `CLOSED`.
    pub state: String,
    pub draft: bool,
    /// `MERGEABLE`, `CONFLICTING`, or `UNKNOWN` while GitHub works it out.
    pub mergeable: String,
    /// `mergeStateStatus` as GitHub says it: `CLEAN`, `HAS_HOOKS`, `UNSTABLE`, `BLOCKED`,
    /// `BEHIND`, `DIRTY` or `UNKNOWN`. The box tells more of them apart than the board's `Merge`.
    pub merge_state: String,
    /// `APPROVED`, `CHANGES_REQUESTED` or `REVIEW_REQUIRED`; None where the repo needs no review.
    pub review_decision: Option<String>,
    /// You can write to the repo, which merging needs.
    pub can_merge: bool,
    /// The methods the repo allows, in GitHub's order.
    pub methods: Vec<Method>,
    /// The one GitHub picks for you in this repo, when the repo allows it.
    pub default_method: Option<Method>,
    pub merge_text: CommitText,
    pub squash_text: CommitText,
    /// The emails a commit you make can be under, and the one GitHub picks.
    pub emails: Vec<String>,
    pub default_email: Option<String>,
    /// You can take it out of draft.
    pub can_ready: bool,
    /// You can merge it before its requirements are met, past the rules.
    pub can_bypass: bool,
    pub can_auto: bool,
    pub can_stop_auto: bool,
    pub auto: Option<AutoMerge>,
    /// Its branch is behind its base and you can bring it up to date.
    pub can_update_branch: bool,
    /// Its base branch merges through a queue.
    pub queue: bool,
    pub queued: Option<Queued>,
    /// GitHub deletes the branch itself once it's merged.
    pub deletes_branch: bool,
    /// The branch's ref, while it's there and you can push to its repo: what deleting it names.
    pub branch_ref: Option<String>,
    pub merged_by: Option<String>,
}

/// Write access or more, as `viewerPermission` says it.
fn writes(permission: Option<&str>) -> bool {
    matches!(permission, Some("WRITE" | "MAINTAIN" | "ADMIN"))
}

pub fn merge_box(slug: &str, number: u64) -> Result<MergeBox> {
    let (owner, name) = slug.split_once('/').ok_or_else(|| anyhow!("bad repo {slug}"))?;
    let args = [format!("query={QUERY}"), format!("owner={owner}"), format!("name={name}"), format!("number={number}")];
    let out = procs::run("gh", &["api", "graphql", "-f", &args[0], "-f", &args[1], "-f", &args[2], "-F", &args[3]], None, Duration::from_secs(30)).map_err(|e| anyhow!("GitHub: {e}"))?;
    parse_box(&out)
}

pub fn parse_box(json: &str) -> Result<MergeBox> {
    let v: Value = serde_json::from_str(json)?;
    let repo = v.pointer("/data/repository").filter(|r| !r.is_null()).ok_or_else(|| anyhow!("GitHub didn't return the repo"))?;
    let pr = repo.get("pullRequest").filter(|p| !p.is_null()).ok_or_else(|| anyhow!("GitHub didn't return the PR"))?;
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(String::from);
    let b = |v: &Value, k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
    let allowed = [("mergeCommitAllowed", Method::Merge), ("squashMergeAllowed", Method::Squash), ("rebaseMergeAllowed", Method::Rebase)];
    let methods: Vec<Method> = allowed.into_iter().filter(|(k, _)| b(repo, k)).map(|(_, m)| m).collect();
    let picked = Method::of(repo.get("viewerDefaultMergeMethod").and_then(Value::as_str));
    let default_method = picked.filter(|m| methods.contains(m)).or(methods.first().copied());
    let text = |title: &str, body: &str| CommitText { title: s(pr, title).unwrap_or_default(), body: s(pr, body).unwrap_or_default() };
    let emails: Vec<String> = repo.get("viewerPossibleCommitEmails").and_then(Value::as_array).into_iter().flatten().filter_map(|e| e.as_str().map(String::from)).collect();
    let auto = pr.get("autoMergeRequest").filter(|a| !a.is_null()).map(|a| AutoMerge {
        method: Method::of(a.get("mergeMethod").and_then(Value::as_str)),
        by: a.pointer("/enabledBy/login").and_then(Value::as_str).map(String::from),
        at: a.get("enabledAt").and_then(Value::as_str).and_then(parse_rfc3339_ms),
    });
    let queued = pr.get("mergeQueueEntry").filter(|q| !q.is_null()).map(|q| Queued { position: q.get("position").and_then(Value::as_u64), state: s(q, "state") });
    let pushes = writes(pr.pointer("/headRepository/viewerPermission").and_then(Value::as_str));
    Ok(MergeBox {
        id: s(pr, "id").ok_or_else(|| anyhow!("GitHub didn't say the PR's id"))?,
        head: s(pr, "headRefOid").unwrap_or_default(),
        branch: s(pr, "headRefName").unwrap_or_default(),
        state: s(pr, "state").unwrap_or_else(|| "OPEN".into()),
        draft: b(pr, "isDraft"),
        mergeable: s(pr, "mergeable").unwrap_or_else(|| "UNKNOWN".into()),
        merge_state: s(pr, "mergeStateStatus").unwrap_or_else(|| "UNKNOWN".into()),
        review_decision: s(pr, "reviewDecision"),
        can_merge: writes(repo.get("viewerPermission").and_then(Value::as_str)),
        methods,
        default_method,
        merge_text: text("mergeTitle", "mergeBody"),
        squash_text: text("squashTitle", "squashBody"),
        default_email: s(repo, "viewerDefaultCommitEmail").filter(|e| emails.contains(e)).or(emails.first().cloned()),
        emails,
        can_ready: b(pr, "viewerCanUpdate"),
        can_bypass: b(pr, "viewerCanMergeAsAdmin"),
        can_auto: b(pr, "viewerCanEnableAutoMerge"),
        can_stop_auto: b(pr, "viewerCanDisableAutoMerge"),
        auto,
        can_update_branch: b(pr, "viewerCanUpdateBranch"),
        queue: b(pr, "isMergeQueueEnabled"),
        queued: queued.filter(|_| b(pr, "isInMergeQueue")),
        deletes_branch: b(repo, "deleteBranchOnMerge"),
        branch_ref: pr.pointer("/headRef/id").and_then(Value::as_str).filter(|_| pushes).map(String::from),
        merged_by: pr.pointer("/mergedBy/login").and_then(Value::as_str).map(String::from),
    })
}

/// What a merge or auto-merge writes: the method, and for a merge commit or a squash the
/// commit's title, message and email (None leaves each to GitHub).
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub method: Method,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

/// One thing the box does, as a window asks it.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum Op {
    /// Merges it now: past the rules too, for one who may.
    Merge {
        #[serde(flatten)]
        commit: Commit,
        head: String,
        /// The branch's ref, to delete once it's merged.
        #[serde(default)]
        delete_branch: Option<String>,
    },
    /// Merges it once its requirements are met.
    Auto {
        #[serde(flatten)]
        commit: Commit,
        head: String,
    },
    StopAuto,
    /// Into the queue its base branch merges through, which picks the method.
    Enqueue { head: String },
    Dequeue,
    /// Brings the branch up to date with its base, by a merge commit or a rebase.
    UpdateBranch { rebase: bool, head: String },
    /// Takes it out of draft.
    Ready,
    DeleteBranch { branch_ref: String },
}

impl Op {
    /// What was done, for a message: "merge" in "Couldn't merge #12".
    fn verb(&self) -> &'static str {
        match self {
            Op::Merge { .. } => "merge",
            Op::Auto { .. } => "turn on auto-merge for",
            Op::StopAuto => "turn off auto-merge for",
            Op::Enqueue { .. } => "queue",
            Op::Dequeue => "take out of the queue",
            Op::UpdateBranch { .. } => "update the branch of",
            Op::Ready => "mark ready for review",
            Op::DeleteBranch { .. } => "delete the branch of",
        }
    }
}

const MERGE: &str = "mutation($pr: ID!, $method: PullRequestMergeMethod!, $title: String, $body: String, $email: String, $head: GitObjectID!) { mergePullRequest(input: {pullRequestId: $pr, mergeMethod: $method, commitHeadline: $title, commitBody: $body, authorEmail: $email, expectedHeadOid: $head}) { pullRequest { state } } }";
const AUTO: &str = "mutation($pr: ID!, $method: PullRequestMergeMethod!, $title: String, $body: String, $email: String, $head: GitObjectID!) { enablePullRequestAutoMerge(input: {pullRequestId: $pr, mergeMethod: $method, commitHeadline: $title, commitBody: $body, authorEmail: $email, expectedHeadOid: $head}) { pullRequest { id } } }";
const STOP_AUTO: &str = "mutation($pr: ID!) { disablePullRequestAutoMerge(input: {pullRequestId: $pr}) { pullRequest { id } } }";
const ENQUEUE: &str = "mutation($pr: ID!, $head: GitObjectID!) { enqueuePullRequest(input: {pullRequestId: $pr, expectedHeadOid: $head}) { mergeQueueEntry { id } } }";
const DEQUEUE: &str = "mutation($pr: ID!) { dequeuePullRequest(input: {id: $pr}) { mergeQueueEntry { id } } }";
const UPDATE_BRANCH: &str = "mutation($pr: ID!, $head: GitObjectID!, $how: PullRequestBranchUpdateMethod!) { updatePullRequestBranch(input: {pullRequestId: $pr, expectedHeadOid: $head, updateMethod: $how}) { pullRequest { id } } }";
const READY: &str = "mutation($pr: ID!) { markPullRequestReadyForReview(input: {pullRequestId: $pr}) { pullRequest { id } } }";
const DELETE_REF: &str = "mutation($ref: ID!) { deleteRef(input: {refId: $ref}) { clientMutationId } }";

/// The arguments `gh` is given for one mutation: the document and its variables, each written
/// as it is (`-f`, never read from a file), and a variable left out is left to GitHub.
fn mutation_args(query: &str, vars: &[(&str, Option<&str>)]) -> Vec<String> {
    let mut args: Vec<String> = vec!["api".into(), "graphql".into(), "-f".into(), format!("query={query}")];
    for (name, value) in vars {
        if let Some(value) = value {
            args.push("-f".into());
            args.push(format!("{name}={value}"));
        }
    }
    args
}

/// The `gh` arguments for each mutation an op sends, in order: a merge that deletes the branch
/// sends two.
pub fn op_args(id: &str, op: &Op) -> Vec<Vec<String>> {
    let pr = ("pr", Some(id));
    let commit = |c: &Commit, head: &str, query: &str| {
        // A rebase writes no commit of its own, so it's given no title or message.
        let text = |t: &Option<String>| t.as_deref().filter(|_| c.method.writes()).map(str::trim_end).filter(|t| !t.is_empty()).map(String::from);
        let (title, body) = (text(&c.title), c.body.as_deref().filter(|_| c.method.writes()).map(str::trim_end).map(String::from));
        let email = c.email.as_deref().filter(|e| !e.is_empty());
        mutation_args(query, &[pr, ("method", Some(c.method.graphql())), ("title", title.as_deref()), ("body", body.as_deref()), ("email", email), ("head", Some(head))])
    };
    match op {
        Op::Merge { commit: c, head, delete_branch } => {
            let mut all = vec![commit(c, head, MERGE)];
            if let Some(r) = delete_branch {
                all.push(mutation_args(DELETE_REF, &[("ref", Some(r))]));
            }
            all
        }
        Op::Auto { commit: c, head } => vec![commit(c, head, AUTO)],
        Op::StopAuto => vec![mutation_args(STOP_AUTO, &[pr])],
        Op::Enqueue { head } => vec![mutation_args(ENQUEUE, &[pr, ("head", Some(head))])],
        Op::Dequeue => vec![mutation_args(DEQUEUE, &[pr])],
        Op::UpdateBranch { rebase, head } => vec![mutation_args(UPDATE_BRANCH, &[pr, ("head", Some(head)), ("how", Some(if *rebase { "REBASE" } else { "MERGE" }))])],
        Op::Ready => vec![mutation_args(READY, &[pr])],
        Op::DeleteBranch { branch_ref } => vec![mutation_args(DELETE_REF, &[("ref", Some(branch_ref))])],
    }
}

/// What an act did: whether GitHub says the PR is merged now, and what went wrong after (a
/// branch that stayed).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Acted {
    pub merged: bool,
    pub note: Option<String>,
}

/// Does what the box asked to PR `number`, whose node id is `id`. A merge that went through but
/// whose branch couldn't be deleted is done, with what went wrong said after.
pub fn act(number: u64, id: &str, op: &Op) -> Result<Acted> {
    let mut steps = op_args(id, op).into_iter();
    let run = |args: Vec<String>| {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        // GitHub can take a while over a merge in a big repo.
        procs::run("gh", &args, None, Duration::from_secs(60))
    };
    let first = steps.next().ok_or_else(|| anyhow!("Nothing to do"))?;
    let out = run(first).map_err(|e| anyhow!("Couldn't {} #{number}: {}", op.verb(), plain(&e)))?;
    let mut acted = Acted { merged: matches!(op, Op::Merge { .. }) && says_merged(&out), note: None };
    for step in steps {
        if let Err(e) = run(step) {
            acted.note = Some(format!("Merged #{number}, but couldn't delete its branch: {}", plain(&e)));
            break;
        }
    }
    Ok(acted)
}

/// Whether `mergePullRequest`'s answer has the PR merged.
fn says_merged(out: &str) -> bool {
    serde_json::from_str::<Value>(out).ok().and_then(|v| v.pointer("/data/mergePullRequest/pullRequest/state").and_then(Value::as_str).map(|s| s == "MERGED")).unwrap_or(false)
}

/// What GitHub said, without `gh`'s name before it.
fn plain(e: &anyhow::Error) -> String {
    let text = e.to_string();
    let text = text.strip_prefix("gh ").unwrap_or(&text);
    text.strip_prefix("gh: ").unwrap_or(text).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX: &str = r#"{"data":{"repository":{"viewerPermission":"ADMIN","mergeCommitAllowed":true,"squashMergeAllowed":true,"rebaseMergeAllowed":false,"deleteBranchOnMerge":false,
      "viewerDefaultMergeMethod":"REBASE","viewerDefaultCommitEmail":"ana@example.com","viewerPossibleCommitEmails":["ana@work.example","ana@example.com"],
      "pullRequest":{"id":"PR_1","state":"OPEN","isDraft":false,"mergeable":"MERGEABLE","mergeStateStatus":"BLOCKED","reviewDecision":"REVIEW_REQUIRED",
        "headRefOid":"abc123","headRefName":"teams","headRef":{"id":"REF_1"},"headRepository":{"viewerPermission":"WRITE"},
        "viewerCanUpdate":true,"viewerCanMergeAsAdmin":true,"viewerCanEnableAutoMerge":true,"viewerCanDisableAutoMerge":false,"viewerCanUpdateBranch":false,
        "isMergeQueueEnabled":false,"isInMergeQueue":false,"mergeQueueEntry":null,"autoMergeRequest":null,
        "mergeTitle":"Merge pull request #7 from ana/teams","mergeBody":"Add teams",
        "squashTitle":"Add teams (#7)","squashBody":"* Add teams\n\n* Name them","mergedBy":null}}}}"#;

    #[test]
    fn reads_what_the_box_offers() {
        let b = parse_box(BOX).unwrap();
        assert_eq!((b.id.as_str(), b.head.as_str(), b.branch.as_str()), ("PR_1", "abc123", "teams"));
        assert_eq!(b.methods, vec![Method::Merge, Method::Squash]);
        // GitHub's pick for you is one the repo no longer allows: the first it does.
        assert_eq!(b.default_method, Some(Method::Merge));
        assert_eq!(b.squash_text, CommitText { title: "Add teams (#7)".into(), body: "* Add teams\n\n* Name them".into() });
        assert_eq!(b.default_email.as_deref(), Some("ana@example.com"));
        assert!(b.can_merge && b.can_bypass && b.can_auto && !b.can_stop_auto);
        assert_eq!(b.branch_ref.as_deref(), Some("REF_1"));
        assert_eq!((b.auto, b.queued), (None, None));
    }

    #[test]
    fn a_branch_goes_only_from_a_repo_you_can_push_to() {
        let read_only = BOX.replace(r#""headRepository":{"viewerPermission":"WRITE"}"#, r#""headRepository":{"viewerPermission":"READ"}"#);
        assert_eq!(parse_box(&read_only).unwrap().branch_ref, None);
        // A branch already deleted, or a fork that's gone.
        let gone = BOX.replace(r#""headRef":{"id":"REF_1"},"headRepository":{"viewerPermission":"WRITE"}"#, r#""headRef":null,"headRepository":null"#);
        assert_eq!(parse_box(&gone).unwrap().branch_ref, None);
        // Triage can't merge.
        let triage = BOX.replace(r#""viewerPermission":"ADMIN""#, r#""viewerPermission":"TRIAGE""#);
        assert!(!parse_box(&triage).unwrap().can_merge);
    }

    #[test]
    fn reads_auto_merge_and_the_queue() {
        let on = BOX
            .replace(r#""autoMergeRequest":null"#, r#""autoMergeRequest":{"mergeMethod":"SQUASH","enabledAt":"2026-10-09T10:00:00Z","enabledBy":{"login":"ana"}}"#)
            .replace(r#""isInMergeQueue":false,"mergeQueueEntry":null"#, r#""isInMergeQueue":true,"mergeQueueEntry":{"position":2,"state":"AWAITING_CHECKS"}"#);
        let b = parse_box(&on).unwrap();
        let auto = b.auto.unwrap();
        assert_eq!((auto.method, auto.by.as_deref()), (Some(Method::Squash), Some("ana")));
        assert!(auto.at.is_some());
        assert_eq!(b.queued, Some(Queued { position: Some(2), state: Some("AWAITING_CHECKS".into()) }));
    }

    #[test]
    fn what_goes_to_another_device_reads_as_codealong_peers_and_back() {
        use codealong_peer::pr;
        // The box another device asks for reads as codealong-peer's, every field of it.
        let on = BOX.replace(r#""autoMergeRequest":null"#, r#""autoMergeRequest":{"mergeMethod":"SQUASH","enabledAt":"2026-10-09T10:00:00Z","enabledBy":{"login":"ana"}}"#);
        let engine = serde_json::to_value(parse_box(&on).unwrap()).unwrap();
        let peer: pr::MergeBox = serde_json::from_value(engine.clone()).expect("the box reads as codealong-peer's");
        assert_eq!(serde_json::to_value(&peer).unwrap(), engine);
        // Each act another device sends reads as the engine's.
        let commit = pr::Commit { method: pr::Method::Squash, title: Some("Add teams (#7)".into()), body: Some(String::new()), email: None };
        let ops = [
            (pr::Op::Merge { commit: commit.clone(), head: "abc".into(), delete_branch: Some("REF_1".into()) }, Op::Merge { commit: Commit { method: Method::Squash, title: Some("Add teams (#7)".into()), body: Some(String::new()), email: None }, head: "abc".into(), delete_branch: Some("REF_1".into()) }),
            (pr::Op::Auto { commit, head: "abc".into() }, Op::Auto { commit: Commit { method: Method::Squash, title: Some("Add teams (#7)".into()), body: Some(String::new()), email: None }, head: "abc".into() }),
            (pr::Op::StopAuto, Op::StopAuto),
            (pr::Op::Enqueue { head: "abc".into() }, Op::Enqueue { head: "abc".into() }),
            (pr::Op::Dequeue, Op::Dequeue),
            (pr::Op::UpdateBranch { rebase: true, head: "abc".into() }, Op::UpdateBranch { rebase: true, head: "abc".into() }),
            (pr::Op::Ready, Op::Ready),
            (pr::Op::DeleteBranch { branch_ref: "REF_1".into() }, Op::DeleteBranch { branch_ref: "REF_1".into() }),
        ];
        for (sent, read) in ops {
            assert_eq!(serde_json::from_value::<Op>(serde_json::to_value(&sent).unwrap()).unwrap(), read);
        }
    }

    #[test]
    fn a_missing_pr_is_an_error() {
        assert!(parse_box(r#"{"data":{"repository":{"pullRequest":null}}}"#).is_err());
        assert!(parse_box(r#"{"data":{"repository":null}}"#).is_err());
    }

    fn op(json: &str) -> Op {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn reads_ops_as_the_windows_send_them() {
        assert_eq!(
            op(r#"{"op":"merge","method":"squash","title":"Add teams (#7)","body":"","email":null,"head":"abc","deleteBranch":"REF_1"}"#),
            Op::Merge { commit: Commit { method: Method::Squash, title: Some("Add teams (#7)".into()), body: Some(String::new()), email: None }, head: "abc".into(), delete_branch: Some("REF_1".into()) }
        );
        assert_eq!(op(r#"{"op":"update-branch","rebase":true,"head":"abc"}"#), Op::UpdateBranch { rebase: true, head: "abc".into() });
        assert_eq!(op(r#"{"op":"delete-branch","branchRef":"REF_1"}"#), Op::DeleteBranch { branch_ref: "REF_1".into() });
        assert_eq!(op(r#"{"op":"stop-auto"}"#), Op::StopAuto);
    }

    /// The variables an op's mutations send, by name.
    fn vars(args: &[String]) -> Vec<String> {
        args.chunks(2).skip(1).map(|p| p[1].clone()).filter(|v| !v.starts_with("query=")).collect()
    }

    #[test]
    fn a_squash_sends_what_was_written_and_the_head_it_saw() {
        let o = op(r#"{"op":"merge","method":"squash","title":"Add teams (#7)  ","body":"* Add teams\n","email":"ana@example.com","head":"abc"}"#);
        let all = op_args("PR_1", &o);
        assert_eq!(all.len(), 1);
        assert_eq!(&all[0][..3], ["api", "graphql", "-f"]);
        assert!(all[0][3].starts_with("query=mutation") && all[0][3].contains("mergePullRequest"));
        assert_eq!(vars(&all[0][2..]), ["pr=PR_1", "method=SQUASH", "title=Add teams (#7)", "body=* Add teams", "email=ana@example.com", "head=abc"]);
    }

    #[test]
    fn an_empty_message_is_sent_empty_and_an_empty_title_left_to_github() {
        let o = op(r#"{"op":"merge","method":"merge","title":"","body":"","head":"abc"}"#);
        assert_eq!(vars(&op_args("PR_1", &o)[0][2..]), ["pr=PR_1", "method=MERGE", "body=", "head=abc"]);
    }

    #[test]
    fn a_rebase_writes_no_commit_of_its_own() {
        let o = op(r#"{"op":"auto","method":"rebase","title":"x","body":"y","head":"abc"}"#);
        let all = op_args("PR_1", &o);
        assert!(all[0][3].contains("enablePullRequestAutoMerge"));
        assert_eq!(vars(&all[0][2..]), ["pr=PR_1", "method=REBASE", "head=abc"]);
    }

    #[test]
    fn a_merge_that_deletes_the_branch_sends_two() {
        let o = op(r#"{"op":"merge","method":"rebase","head":"abc","deleteBranch":"REF_1"}"#);
        let all = op_args("PR_1", &o);
        assert_eq!(all.len(), 2);
        assert!(all[1][3].contains("deleteRef"));
        assert_eq!(vars(&all[1][2..]), ["ref=REF_1"]);
    }

    #[test]
    fn the_rest_name_the_pr_and_what_they_need() {
        let first = |json: &str| op_args("PR_1", &op(json)).remove(0);
        let update = first(r#"{"op":"update-branch","rebase":false,"head":"abc"}"#);
        assert!(update[3].contains("updatePullRequestBranch"));
        assert_eq!(vars(&update[2..]), ["pr=PR_1", "head=abc", "how=MERGE"]);
        assert_eq!(vars(&first(r#"{"op":"enqueue","head":"abc"}"#)[2..]), ["pr=PR_1", "head=abc"]);
        for (json, mutation) in [(r#"{"op":"dequeue"}"#, "dequeuePullRequest"), (r#"{"op":"stop-auto"}"#, "disablePullRequestAutoMerge"), (r#"{"op":"ready"}"#, "markPullRequestReadyForReview")] {
            let args = first(json);
            assert!(args[3].contains(mutation), "{json}");
            assert_eq!(vars(&args[2..]), ["pr=PR_1"]);
        }
    }

    #[test]
    fn a_merge_is_merged_when_github_says_so() {
        assert!(says_merged(r#"{"data":{"mergePullRequest":{"pullRequest":{"state":"MERGED"}}}}"#));
        assert!(!says_merged(r#"{"data":{"mergePullRequest":{"pullRequest":{"state":"OPEN"}}}}"#));
        assert!(!says_merged(""));
    }

    #[test]
    fn says_what_github_refused_and_a_branch_that_stayed() {
        use std::cell::RefCell;
        use std::rc::Rc;
        let sent = Rc::new(RefCell::new(Vec::new()));
        let log = sent.clone();
        // As `procs::run` says what `gh` printed when GitHub refused a mutation.
        let refused = |why: &str| Err(anyhow!("gh gh: {why}"));
        let merged = || Ok(r#"{"data":{"mergePullRequest":{"pullRequest":{"state":"MERGED"}}}}"#.to_string());
        let mut answers = vec![refused("Head branch was modified. Review and try the merge again."), merged(), Ok(String::new()), merged(), refused("Reference does not exist")].into_iter();
        procs::stand_in::set(Some(Box::new(move |program, args, _, _| {
            log.borrow_mut().push((program.to_string(), args.iter().any(|a| a.contains("deleteRef"))));
            answers.next().unwrap()
        })));
        let o = op(r#"{"op":"merge","method":"squash","head":"abc","deleteBranch":"REF_1"}"#);
        // The merge refused: the branch isn't touched.
        assert_eq!(act(7, "PR_1", &o).unwrap_err().to_string(), "Couldn't merge #7: Head branch was modified. Review and try the merge again.");
        assert_eq!(sent.borrow().len(), 1);
        // Merged and deleted.
        assert_eq!(act(7, "PR_1", &o).unwrap(), Acted { merged: true, note: None });
        // Merged, the branch stayed.
        assert_eq!(act(7, "PR_1", &o).unwrap(), Acted { merged: true, note: Some("Merged #7, but couldn't delete its branch: Reference does not exist".into()) });
        procs::stand_in::set(None);
        assert_eq!(sent.borrow().iter().filter(|(p, _)| p == "gh").count(), 5);
    }
}
