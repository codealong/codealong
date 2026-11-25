//! Experiments: running pinned agent combinations side-by-side on the same task and base commit,
//! optionally with a 1-round local reviewer, a mechanical gate command, and a blind pairwise judge.
//!
//! State lives in `experiments.json` (through `store.rs`), artifacts (spec, blinded diffs, review
//! notes, gate logs) under `support_dir()/experiments/<run_id>/`, and session costs are priced on
//! demand from the token ledger (`ledger::session_in`) so nothing duplicates SQLite rows.
//!
//! Each arm is a `Single` topology (`worker` + optional `reviewer` seat) tagged with `ArmKind`
//! (with `#[serde(other)] Unknown` for forward compatibility across Codealong and Codealong Dev).
//! Future topologies (`lead-workers`, `pipeline`, `cycle`) extend `ArmKind` without rewriting runs.

pub mod gitops;
pub mod prompts;
pub mod report;
pub mod runner;

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

use crate::boundary;
use crate::config::{support_dir, AccountCfg, Config};
use crate::harness;
use crate::start::{self, Place};
use crate::store;
use crate::util::{home, now_ms};

const WHAT: &str = "experiments";

/// Where live and finished experiment runs are stored.
pub fn path() -> PathBuf {
    support_dir().join("experiments.json")
}

/// Artifact directory for one run (`spec.md`, per-trial patches/reviews, isolated judge dirs).
pub fn folder(run_id: &str) -> PathBuf {
    support_dir().join("experiments").join(run_id)
}

/// A pinned account, model, effort and optional permission mode for one role in a run.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Seat {
    pub account: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// Optional ACP permission mode id (`acp::switch_mode`) set before the first prompt runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

impl Seat {
    /// Human-readable compact form (`claude/opus/high`).
    pub fn short(&self) -> String {
        let mut out = self.account.clone();
        if let Some(m) = self.model.as_deref().filter(|s| !s.is_empty()) {
            out.push('/');
            out.push_str(m);
        }
        if let Some(e) = self.effort.as_deref().filter(|s| !s.is_empty()) {
            out.push('/');
            out.push_str(e);
        }
        out
    }
}

/// Shape of an arm's topology. Unknown variants deserialize safely so older builds sharing the
/// data folder can still read `experiments.json`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ArmKind {
    #[default]
    Single,
    #[serde(other)]
    Unknown,
}

/// One candidate combination in a run.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Arm {
    pub label: String,
    pub kind: ArmKind,
    pub worker: Seat,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewer: Option<Seat>,
}

/// Mechanical verification command run in each trial's worktree after its last agent turn.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Gate {
    pub cmd: String,
    pub timeout_secs: u64,
    /// Run the gate once on a clean base worktree before spending tokens on arms.
    pub check_base: bool,
}

impl Default for Gate {
    fn default() -> Self {
        Self { cmd: String::new(), timeout_secs: 600, check_base: true }
    }
}

/// Blind pairwise judge configuration.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct JudgeCfg {
    pub seat: Seat,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RunStage {
    #[default]
    Running,
    Judging,
    Complete,
    Cancelled,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TrialStage {
    #[default]
    Queued,
    /// Worktree created on disk; waiting for the engine's board to list its lane.
    Preparing,
    Working,
    Reviewing,
    Fixing,
    Gating,
    Ready,
    Failed,
    Voided,
    #[serde(other)]
    Unknown,
}

impl TrialStage {
    pub fn is_terminal(self) -> bool {
        matches!(self, TrialStage::Ready | TrialStage::Failed | TrialStage::Voided)
    }

    pub fn is_active(self) -> bool {
        matches!(self, TrialStage::Preparing | TrialStage::Working | TrialStage::Reviewing | TrialStage::Fixing | TrialStage::Gating)
    }
}

/// Phase timestamps (epoch ms) recorded as a trial moves through its stages.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct PhaseTimes {
    pub queued_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_done_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_done_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_done_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate_done_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<i64>,
}

/// One session spawned for a trial (`worker`, `reviewer`, `judge`, etc.).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct TrialSession {
    pub key: String,
    pub role: String,
    pub step: u8,
    pub started_at: i64,
}

/// One arm executing in one isolated worktree for a run.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Trial {
    pub id: String,
    pub arm: Arm,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub stage: TrialStage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewer_key: Option<String>,
    /// Epoch ms when the current agent turn was sent, for `turn_ended` gating.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_sent_at: Option<i64>,
    pub sessions: Vec<TrialSession>,
    pub times: PhaseTimes,
    pub interruptions: u32,
    /// Git ref storing the pre-gate snapshot commit (`refs/codealong/experiments/<run>/<trial>`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff_ref: Option<String>,
    pub diff_bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree_before_review: Option<String>,
    pub reviewer_modified_tree: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_verdict: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate_passed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate_exit: Option<i32>,
    pub pr_opened: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drift: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub adopted: bool,
    pub discarded: bool,
}

impl Trial {
    /// Whether this trial survived to compete in judging (finished cleanly with a non-empty diff
    /// and passed the gate when one was configured).
    pub fn survives(&self) -> bool {
        self.stage == TrialStage::Ready && self.diff_bytes > 0 && self.gate_passed != Some(false)
    }

    pub fn record_session(&mut self, key: String, role: &str, step: u8, at: i64) {
        if !self.sessions.iter().any(|s| s.key == key) {
            self.sessions.push(TrialSession { key, role: role.to_string(), step, started_at: at });
        }
    }
}

/// One ordered judge call (`Candidate A` vs `Candidate B`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct PairOrder {
    pub a_trial: String,
    pub b_trial: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    /// `"A"`, `"B"`, or `"tie"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub done: bool,
}

impl PairOrder {
    /// Maps `"A"` / `"B"` back to the winning trial id (`None` for tie or missing).
    pub fn winning_trial(&self) -> Option<&str> {
        match self.winner.as_deref() {
            Some("A") => Some(self.a_trial.as_str()),
            Some("B") => Some(self.b_trial.as_str()),
            _ => None,
        }
    }
}

/// Both orderings of a blind pairwise comparison between two surviving trials.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct PairComparison {
    pub trial_a: String,
    pub trial_b: String,
    pub forward: PairOrder,
    pub reversed: PairOrder,
    /// `Some(trial_id)` only when both orderings independently pick the same trial.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consistent_winner: Option<String>,
}

impl PairComparison {
    pub fn new(trial_a: String, trial_b: String) -> Self {
        Self {
            forward: PairOrder { a_trial: trial_a.clone(), b_trial: trial_b.clone(), ..Default::default() },
            reversed: PairOrder { a_trial: trial_b.clone(), b_trial: trial_a.clone(), ..Default::default() },
            trial_a,
            trial_b,
            consistent_winner: None,
        }
    }

    pub fn is_done(&self) -> bool {
        self.forward.done && self.reversed.done
    }

    pub fn resolve(&mut self) {
        if !self.is_done() {
            return;
        }
        let fwd = self.forward.winning_trial();
        let rev = self.reversed.winning_trial();
        self.consistent_winner = match (fwd, rev) {
            (Some(f), Some(r)) if f == r => Some(f.to_string()),
            _ => None,
        };
    }
}

/// One bake-off run executing multiple arms on the same task and base commit.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    pub experiment: String,
    pub repo: String,
    pub main_worktree: String,
    pub base_sha: String,
    pub prompt: String,
    pub max_parallel: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate: Option<Gate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_gate_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub judge: Option<JudgeCfg>,
    pub stage: RunStage,
    pub trials: Vec<Trial>,
    pub comparisons: Vec<PairComparison>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommendation_note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adopted_trial: Option<String>,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct StoreFile {
    pub runs: Vec<Run>,
}

/// Operations asked from the CLI or windows (`calls.rs::experiment_op`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum Op {
    Adopt { run_id: String, trial_id: String },
    Discard { run_id: String, trial_id: Option<String> },
    Cancel { run_id: String },
}

pub fn load() -> Result<StoreFile> {
    Ok(store::read(&path(), WHAT)?.unwrap_or_default())
}

pub fn change<F>(f: F) -> Result<()>
where
    F: FnOnce(&mut StoreFile) -> bool,
{
    store::change(&path(), WHAT, f).map(|_| ())
}

/// Generates a short readable id (`exp-` + 8 hex chars).
pub fn new_id(prefix: &str) -> String {
    let mut b = [0u8; 4];
    crate::os::system::random_bytes(&mut b);
    format!("{prefix}-{:02x}{:02x}{:02x}{:02x}", b[0], b[1], b[2], b[3])
}

/// Parses a seat spec `<account>[:<model>][:<effort>][@<mode>]`.
/// Account is before the first `:`; effort is stripped after the last `:` ONLY when the account's
/// adapter lists that effort, so model ids containing colons (`llama3:8b`, `model:free`) stay
/// intact.
pub fn parse_seat(cfg: &Config, raw: &str) -> Result<Seat> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("Seat can't be empty. Use <account>[:<model>][:<effort>][@<mode>].");
    }
    let (body, mode) = match trimmed.rsplit_once('@') {
        Some((left, m)) if !m.trim().is_empty() => (left.trim(), Some(m.trim().to_string())),
        _ => (trimmed, None),
    };
    let (acct_name, rest) = match body.split_once(':') {
        Some((a, r)) => (a.trim(), Some(r.trim())),
        None => (body.trim(), None),
    };
    if acct_name.eq_ignore_ascii_case("auto") || cfg.routing.groups.iter().any(|g| g.name.trim() == acct_name) {
        bail!("An experiment arm must pin an account, not {acct_name:?}.");
    }
    let account = cfg.accounts.iter().find(|a| a.name == acct_name).ok_or_else(|| anyhow!("There's no account named {acct_name:?}."))?;
    account.ensure_on()?;
    let adapter = harness::of(account).ok_or_else(|| anyhow!("Unknown harness for {acct_name:?}."))?;
    let efforts = adapter.efforts();
    let (model, effort) = match rest.filter(|r| !r.is_empty()) {
        None => (None, None),
        Some(r) => {
            if let Some((left, tail)) = r.rsplit_once(':') {
                if efforts.iter().any(|(id, _)| *id == tail) {
                    let m = (!left.is_empty()).then(|| left.to_string());
                    (m, Some(tail.to_string()))
                } else {
                    (Some(r.to_string()), None)
                }
            } else if efforts.iter().any(|(id, _)| *id == r) {
                (None, Some(r.to_string()))
            } else {
                (Some(r.to_string()), None)
            }
        }
    };
    if let Some(m) = model.as_deref() {
        if !start::valid_model(m) {
            bail!("{m:?} isn't a valid model name for {acct_name}.");
        }
    }
    Ok(Seat { account: acct_name.to_string(), model, effort, mode })
}

/// Parses an arm spec `<worker_seat>[+<reviewer_seat>]`.
pub fn parse_arm(cfg: &Config, raw: &str) -> Result<Arm> {
    let parts: Vec<&str> = raw.split('+').map(str::trim).filter(|s| !s.is_empty()).collect();
    if parts.is_empty() || parts.len() > 2 {
        bail!("Arm {raw:?} should be <worker> or <worker>+<reviewer>.");
    }
    let worker = parse_seat(cfg, parts[0])?;
    let reviewer = parts.get(1).map(|s| parse_seat(cfg, s)).transpose()?;
    let label = match &reviewer {
        Some(r) => format!("{} + {}", worker.short(), r.short()),
        None => worker.short(),
    };
    Ok(Arm { label, kind: ArmKind::Single, worker, reviewer })
}

/// Checks that every account in `accounts` belongs to the same boundary group, or none of them
/// does: sharing specs, diffs or reviews across a boundary would leak what `boundary.rs` keeps in.
pub fn check_boundary(cfg: &Config, accounts: &[&str]) -> Result<()> {
    let mut seen: Option<Option<&str>> = None;
    for acct in accounts {
        let group = boundary::group_of(cfg, acct).map(|g| g.name.trim());
        match seen {
            None => seen = Some(group),
            Some(prev) if prev != group => {
                let describe = |g: Option<&str>| g.map_or("outside any boundary".to_string(), |n| format!("in boundary group {n:?}"));
                bail!("All seats in an experiment run must share the same boundary: {acct:?} is {}, while another seat is {}.", describe(group), describe(prev));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Validates every seat in a run (ACP hosted support, boundary consistency, repo hooks) and returns
/// non-fatal warnings (such as the judge sharing a harness with an arm).
pub fn validate_run(cfg: &Config, main_worktree: &Path, arms: &[Arm], judge: Option<&JudgeCfg>) -> Result<Vec<String>> {
    if arms.len() < 2 {
        bail!("An experiment run needs at least two --arm entries.");
    }
    if crate::worktree::has_create_hook(cfg, main_worktree) {
        bail!("This repository declares a WorktreeCreate hook; experiments can't pin a base commit or safely force-remove hook-managed worktrees yet.");
    }
    let mut all_accounts: Vec<&str> = Vec::new();
    let mut arm_harnesses = HashSet::new();
    let user_home = home();
    for arm in arms {
        for seat in std::iter::once(&arm.worker).chain(arm.reviewer.iter()) {
            let acct = ensure_acp_seat(cfg, seat, &user_home)?;
            all_accounts.push(&acct.name);
        }
        if let Some(a) = cfg.accounts.iter().find(|a| a.name == arm.worker.account) {
            arm_harnesses.insert(a.harness.clone());
        }
    }
    let mut warnings = Vec::new();
    if let Some(j) = judge {
        let acct = ensure_acp_seat(cfg, &j.seat, &user_home)?;
        all_accounts.push(&acct.name);
        if arm_harnesses.contains(&acct.harness) {
            warnings.push(format!("Judge account {} uses the same harness ({}) as an arm; consider a judge on another provider to reduce self-preference bias.", acct.name, acct.harness));
        }
    }
    check_boundary(cfg, &all_accounts)?;
    Ok(warnings)
}

fn ensure_acp_seat<'a>(cfg: &'a Config, seat: &Seat, user_home: &Path) -> Result<&'a AccountCfg> {
    let acct = cfg.accounts.iter().find(|a| a.name == seat.account).ok_or_else(|| anyhow!("There's no account named {:?}.", seat.account))?;
    acct.ensure_on()?;
    let places = start::places_of(acct, user_home).map_err(|e| anyhow!(e))?;
    if !places.contains(&Place::Codealong) {
        bail!("{} can't run hosted in Codealong (no ACP adapter available). Experiments require hosted sessions so worktrees stay pinned.", acct.name);
    }
    Ok(acct)
}

/// Builds pairwise comparisons for every unordered pair of surviving trials.
pub fn build_comparisons(trials: &[Trial]) -> Vec<PairComparison> {
    let survivors: Vec<&str> = trials.iter().filter(|t| t.survives()).map(|t| t.id.as_str()).collect();
    let mut out = Vec::new();
    for i in 0..survivors.len() {
        for j in (i + 1)..survivors.len() {
            out.push(PairComparison::new(survivors[i].to_string(), survivors[j].to_string()));
        }
    }
    out
}

/// Computes the overall recommendation from surviving trials and resolved pairwise comparisons.
/// Recommends a winner only when a single survivor strictly leads in consistent pairwise wins
/// (both orderings agreed) with no head-to-head consistent loss to the runner-up.
pub fn compute_recommendation(trials: &[Trial], comparisons: &[PairComparison]) -> (Option<String>, String) {
    let survivors: Vec<&Trial> = trials.iter().filter(|t| t.survives()).collect();
    match survivors.len() {
        0 => return (None, "No arm produced a passing diff.".into()),
        1 => return (Some(survivors[0].id.clone()), format!("Only surviving arm ({}).", survivors[0].arm.label)),
        _ => {}
    }
    if comparisons.is_empty() {
        return (None, "No judge configured; compare surviving diffs by hand.".into());
    }
    let mut wins: BTreeMap<&str, usize> = survivors.iter().map(|t| (t.id.as_str(), 0)).collect();
    for c in comparisons {
        if let Some(w) = c.consistent_winner.as_deref() {
            if let Some(slot) = wins.get_mut(w) {
                *slot += 1;
            }
        }
    }
    let mut ranked: Vec<(&str, usize)> = wins.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let (top_id, top_wins) = ranked[0];
    let (second_id, second_wins) = ranked[1];
    if top_wins == 0 || top_wins == second_wins {
        return (None, "Too close: pairwise orderings did not agree on a clear winner.".into());
    }
    // Check that the leader didn't lose or draw inconsistently head-to-head against the runner-up.
    let beat_second = comparisons.iter().any(|c| {
        ((c.trial_a == top_id && c.trial_b == second_id) || (c.trial_a == second_id && c.trial_b == top_id))
            && c.consistent_winner.as_deref() == Some(top_id)
    });
    if !beat_second {
        return (None, "Too close: top arms did not have a consistent head-to-head winner.".into());
    }
    let label = survivors.iter().find(|t| t.id == top_id).map_or(top_id, |t| t.arm.label.as_str());
    (Some(top_id.to_string()), format!("Recommended {label} ({top_wins}/{} consistent pairwise wins).", survivors.len() - 1))
}

/// Creates a new `Run` struct with queued trials ready to be saved to `experiments.json`.
pub fn make_run(experiment: String, repo: String, main_worktree: PathBuf, base_sha: String, prompt: String, arms: Vec<Arm>, gate: Option<Gate>, judge: Option<JudgeCfg>, max_parallel: usize) -> Run {
    let now = now_ms();
    let id = new_id("exp");
    let trials = arms
        .into_iter()
        .enumerate()
        .map(|(i, arm)| Trial {
            id: format!("t{}", i + 1),
            arm,
            stage: TrialStage::Queued,
            times: PhaseTimes { queued_at: now, ..Default::default() },
            ..Default::default()
        })
        .collect();
    Run {
        id,
        experiment,
        repo,
        main_worktree: main_worktree.to_string_lossy().into_owned(),
        base_sha,
        prompt,
        max_parallel: max_parallel.max(1),
        gate,
        base_gate_ok: None,
        judge,
        stage: RunStage::Running,
        trials,
        comparisons: Vec::new(),
        recommended: None,
        recommendation_note: None,
        adopted_trial: None,
        created_at: now,
        finished_at: None,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GroupMemberCfg, RoutingGroupCfg};

    fn acct(name: &str, harness: &str, home: &str) -> AccountCfg {
        toml::from_str(&format!("name = \"{name}\"\nharness = \"{harness}\"\nhome = \"{home}\"\n")).unwrap()
    }

    fn test_cfg() -> Config {
        let mut cfg = Config::default();
        cfg.accounts = vec![
            acct("claude", "claude", "~/.claude"),
            acct("codex", "codex", "~/.codex"),
            acct("opencode", "opencode", "~/.local/share/opencode"),
            acct("claude-work", "claude", "~/.claude-work"),
        ];
        cfg.routing.groups.push(RoutingGroupCfg {
            name: "corp".into(),
            boundary: true,
            members: vec![GroupMemberCfg { account: "claude-work".into(), ..Default::default() }],
            ..Default::default()
        });
        cfg
    }

    #[test]
    fn parse_seat_handles_colons_in_models_efforts_and_modes() {
        let cfg = test_cfg();
        assert_eq!(
            parse_seat(&cfg, "claude:opus[1m]:high@bypassPermissions").unwrap(),
            Seat { account: "claude".into(), model: Some("opus[1m]".into()), effort: Some("high".into()), mode: Some("bypassPermissions".into()) }
        );
        // OpenCode has no listed efforts, so `:8b` and `:free` stay part of the model id.
        assert_eq!(
            parse_seat(&cfg, "opencode:llama3:8b").unwrap(),
            Seat { account: "opencode".into(), model: Some("llama3:8b".into()), effort: None, mode: None }
        );
        assert_eq!(
            parse_seat(&cfg, "opencode:model:free").unwrap(),
            Seat { account: "opencode".into(), model: Some("model:free".into()), effort: None, mode: None }
        );
        // Effort alone without model.
        assert_eq!(
            parse_seat(&cfg, "claude:xhigh").unwrap(),
            Seat { account: "claude".into(), model: None, effort: Some("xhigh".into()), mode: None }
        );
        // Auto and routing groups are rejected.
        assert!(parse_seat(&cfg, "auto").is_err());
        assert!(parse_seat(&cfg, "corp").is_err());
    }

    #[test]
    fn parse_arm_splits_worker_and_optional_reviewer() {
        let cfg = test_cfg();
        let arm = parse_arm(&cfg, "codex:sol-6.2:high+claude:opus:high").unwrap();
        assert_eq!(arm.worker.account, "codex");
        assert_eq!(arm.worker.model.as_deref(), Some("sol-6.2"));
        assert_eq!(arm.worker.effort.as_deref(), Some("high"));
        let rev = arm.reviewer.unwrap();
        assert_eq!(rev.account, "claude");
        assert_eq!(rev.model.as_deref(), Some("opus"));
        assert_eq!(arm.label, "codex/sol-6.2/high + claude/opus/high");
    }

    #[test]
    fn boundary_rejects_mixing_boundary_and_non_boundary_seats() {
        let cfg = test_cfg();
        assert!(check_boundary(&cfg, &["claude", "codex"]).is_ok());
        assert!(check_boundary(&cfg, &["claude-work", "claude-work"]).is_ok());
        let err = check_boundary(&cfg, &["claude-work", "codex"]).unwrap_err().to_string();
        assert!(err.contains("same boundary"), "{err}");
    }

    #[test]
    fn unknown_arm_kind_deserializes_for_forward_compat() {
        let arm: Arm = serde_json::from_str(r#"{"label":"future","kind":"lead-workers","worker":{"account":"claude"}}"#).unwrap();
        assert_eq!(arm.kind, ArmKind::Unknown);
    }

    #[test]
    fn recommendation_requires_both_orders_to_agree() {
        let trial = |id: &str| Trial { id: id.into(), arm: Arm { label: id.into(), ..Default::default() }, stage: TrialStage::Ready, diff_bytes: 100, gate_passed: Some(true), ..Default::default() };
        let trials = vec![trial("t1"), trial("t2")];
        // Both orders pick t1 -> recommended.
        let mut c = PairComparison::new("t1".into(), "t2".into());
        c.forward.done = true;
        c.forward.winner = Some("A".into());
        c.reversed.done = true;
        c.reversed.winner = Some("B".into());
        c.resolve();
        assert_eq!(c.consistent_winner.as_deref(), Some("t1"));
        let (rec, _) = compute_recommendation(&trials, &[c]);
        assert_eq!(rec.as_deref(), Some("t1"));

        // Order bias (both pick "A") -> inconsistent -> too close.
        let mut biased = PairComparison::new("t1".into(), "t2".into());
        biased.forward.done = true;
        biased.forward.winner = Some("A".into());
        biased.reversed.done = true;
        biased.reversed.winner = Some("A".into());
        biased.resolve();
        assert_eq!(biased.consistent_winner, None);
        let (rec, note) = compute_recommendation(&trials, &[biased]);
        assert_eq!(rec, None);
        assert!(note.contains("Too close"), "{note}");
    }
}
