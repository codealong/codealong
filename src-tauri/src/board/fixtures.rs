//! What the board's tests start from, for the tests of more than one of its files: the time
//! they call now, a Claude Code account, a live session's registry entry and its transcript, a
//! worktree's seed, a PR as GitHub gave it, and a read of a session's reply. `crate::fixtures`
//! (codealong-peer's) has the plain session and lane the tests build on beside these.
//!
//! Test-only. A helper only one file's tests use stays in that file's `tests`; one that a
//! second file comes to need moves here, and each `tests` names the ones it uses.

use super::*;

pub(super) const NOW: i64 = 1_790_200_000_000;

pub(super) fn acct(name: &str) -> AccountCfg {
    AccountCfg { name: name.into(), harness: "claude".into(), home: format!("~/.{name}"), command: Some(name.into()), start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None }
}

pub(super) fn reg(status: &str, updated: i64) -> RegistryEntry {
    RegistryEntry {
        pid: 1,
        session_id: "abc1234567".into(),
        cwd: "/r/spark-portal/apps/server".into(),
        status: status.into(),
        kind: Some("interactive".into()),
        job_id: None,
        parked_job: None,
        spare: false,
        entrypoint: Some("cli".into()),
        name: Some("spark-portal-8f".into()),
        name_source: Some("derived".into()),
        started_at: Some(NOW - 3_600_000),
        status_updated_at: Some(updated),
        ..Default::default()
    }
}

pub(super) fn tr() -> TranscriptInfo {
    TranscriptInfo {
        session_id: "abc1234567".into(),
        cwd: "/r/spark-portal/apps/server".into(),
        title: Some("Portal daily log + photo sync".into()),
        pending_tool: Some("Bash · `yarn mikro-orm migration:up`".into()),
        last_tool: Some("running `yarn test`".into()),
        last_text: Some("Done.".into()),
        last_ts: NOW - 60_000,
        ..Default::default()
    }
}

pub(super) fn seed(path: &str, branch: &str) -> LaneSeed {
    LaneSeed {
        path: PathBuf::from(path),
        project: "Spruce".into(),
        repo: "spark".into(),
        repo_slug: Some("sprucehq/spark".into()),
        head_owners: vec!["sprucehq".into(), "gwen".into()],
        callsign: "portal".into(),
        origin: Origin::Script,
        branch: Some(branch.into()),
        head: Some("0ef4ed2".into()),
        default_branch: Some("main".into()),
        elsewhere: false,
        chats: false,
        starts_chats: false,
        exact: false,
    }
}

pub(super) fn pr(state: PrState, failed: u32) -> PrRaw {
    PrRaw {
        number: 1037,
        title: "Let the agent add and caption project photos".into(),
        url: "https://github.com/sprucehq/spark/pull/1037".into(),
        state,
        head_ref: "portal".into(),
        head_owner: Some("gwen".into()),
        head_oid: Some("0ef4ed2".into()),
        updated_at: NOW - 3_600_000,
        merged_at: if state == PrState::Merged { Some(NOW - 7_200_000) } else { None },
        closed_at: None,
        checks: Checks { passed: 11, failed, failing: (0..failed).map(|i| format!("check-{i}")).collect(), ..Default::default() },
        merge: Merge::Unknown,
        author: Some("gwen".into()),
        base_ref: Some("main".into()),
        created_at: NOW - 86_400_000,
        additions: 120,
        deletions: 8,
        changed_files: 6,
        review_requested: false,
        requested_by: None,
        requested_at: None,
        reviewers_asked: Vec::new(),
        assigned: None,
        authored_by: None,
        labels: Vec::new(),
        last_review: None,
        stale_approval: None,
        head_at: None,
        said: Vec::new(),
        said_from: None,
        changes_asked: Vec::new(),
    }
}

pub(super) fn insight(f: impl FnOnce(&mut SessionInsight)) -> SessionInsight {
    let mut i = SessionInsight::default();
    f(&mut i);
    i
}
