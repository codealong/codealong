//! Experiments: running pinned agent combinations side-by-side on the same task and base commit,
//! optionally with a 1-round local reviewer, a mechanical gate command, and one blind judge or a
//! panel of them, ranking the results or comparing them in pairs.
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

/// How judges evaluate surviving trials in a run.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum JudgeMode {
    /// All-at-once ranking for 3+ survivors; pairwise (both orders) for 2 survivors.
    #[default]
    Auto,
    /// All surviving diffs evaluated together with cyclic label rotation across judges.
    Rank,
    /// Every unordered pair evaluated in both A/B and B/A order.
    Pairwise,
    #[serde(other)]
    #[value(skip)]
    Unknown,
}

fn is_empty_seat(s: &Seat) -> bool {
    s.account.is_empty()
}

fn is_default_judge_mode(m: &JudgeMode) -> bool {
    *m == JudgeMode::Auto
}

/// Blind judge configuration (single seat or multi-provider panel).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct JudgeCfg {
    /// Primary (or sole) judge seat; kept for backward compatibility with single-judge runs.
    #[serde(skip_serializing_if = "is_empty_seat")]
    pub seat: Seat,
    /// Full panel of judge seats when 2+ judges are configured.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub seats: Vec<Seat>,
    /// `auto` (all-at-once ranking for 3+ survivors, pairwise for 2), `rank`, or `pairwise`.
    #[serde(skip_serializing_if = "is_default_judge_mode")]
    pub mode: JudgeMode,
    /// Optional extra evaluation criteria appended to the blind judge's prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rubric: Option<String>,
}

impl JudgeCfg {
    /// Builds a `JudgeCfg` from 1 or more resolved seats (`None` when `seats` is empty).
    pub fn from_seats(seats: Vec<Seat>, mode: JudgeMode, rubric: Option<String>) -> Option<Self> {
        let first = seats.first()?.clone();
        let panel = if seats.len() > 1 { seats } else { Vec::new() };
        Some(Self { seat: first, seats: panel, mode, rubric })
    }

    /// Returns all configured judge seats in panel order (falling back to `[seat]` for single-judge configs).
    pub fn resolved_seats(&self) -> Vec<&Seat> {
        if !self.seats.is_empty() {
            self.seats.iter().collect()
        } else if !self.seat.account.is_empty() {
            vec![&self.seat]
        } else {
            Vec::new()
        }
    }

    /// Whether judging should use all-at-once ranking (`RankJudge`) instead of pairwise (`PairComparison`).
    pub fn use_rank(&self, survivor_count: usize) -> bool {
        match self.mode {
            JudgeMode::Rank => true,
            JudgeMode::Pairwise => false,
            JudgeMode::Auto | JudgeMode::Unknown => survivor_count >= 3,
        }
    }
}

/// One `[experiments.<name>]` preset in `config.toml`: a reusable set of arms and an optional
/// blind judge or multi-provider judge panel (and rubric) so `codealong exp run --preset <name>`
/// only needs a task prompt. No mechanical `eval` gate is required.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct PresetCfg {
    /// Arm specs (`<worker>[+<reviewer>]`), at least two.
    pub arms: Vec<String>,
    /// Single blind judge seat (`<account>[:<model>][:<effort>][@<mode>]`). Use `judges` for a panel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub judge: Option<String>,
    /// Panel of blind judge seats (e.g. `["claude:opus:high", "codex:gpt-6.2-astra:high", "antigravity:gemini-4-neon-high"]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub judges: Vec<String>,
    /// Judging mode: `auto` (rank for 3+ survivors, pairwise for 2), `rank`, or `pairwise`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub judge_mode: Option<JudgeMode>,
    /// Optional extra evaluation criteria for the blind judge(s).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rubric: Option<String>,
    /// Optional mechanical gate command; omit when not needed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eval: Option<String>,
    /// Optional gate timeout in seconds (defaults to 600 when `eval` is set).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eval_timeout: Option<u64>,
    /// Skip running `eval` on a clean base worktree before launching arms.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip_base_check: bool,
    /// Optional max concurrent trials (defaults to 2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_parallel: Option<usize>,
}

impl PresetCfg {
    /// Resolved judge seat strings from `judges` (when set) or `judge`.
    pub fn resolved_judges(&self) -> Vec<String> {
        if !self.judges.is_empty() {
            self.judges.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
        } else if let Some(j) = self.judge.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            vec![j.to_string()]
        } else {
            Vec::new()
        }
    }
}

/// Removes extracted `judge/*/repo` base-commit snapshots under a run's artifact folder so they
/// don't linger on disk after judging finishes, is cancelled, or is discarded.
pub fn cleanup_judge_repos(run_id: &str) {
    let judge_dir = folder(run_id).join("judge");
    let Ok(entries) = std::fs::read_dir(&judge_dir) else { return };
    for entry in entries.flatten() {
        let repo_dir = entry.path().join("repo");
        if repo_dir.exists() {
            let _ = std::fs::remove_dir_all(&repo_dir);
        }
    }
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

/// Both orderings of a blind pairwise comparison between two surviving trials by one judge seat.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct PairComparison {
    pub trial_a: String,
    pub trial_b: String,
    /// Judge seat evaluating this pair (`None` in legacy single-judge runs, which use `run.judge.seat`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub judge: Option<Seat>,
    pub forward: PairOrder,
    pub reversed: PairOrder,
    /// `Some(trial_id)` only when both orderings independently pick the same trial.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consistent_winner: Option<String>,
}

impl PairComparison {
    pub fn new(trial_a: String, trial_b: String, judge: Option<Seat>) -> Self {
        Self {
            forward: PairOrder { a_trial: trial_a.clone(), b_trial: trial_b.clone(), ..Default::default() },
            reversed: PairOrder { a_trial: trial_b.clone(), b_trial: trial_a.clone(), ..Default::default() },
            trial_a,
            trial_b,
            judge,
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

/// One blind all-at-once ranking pass by a single judge seat over all surviving trials.
/// Labels `A`, `B`, `C`, ... correspond positionally to `order` (cyclically rotated across judges
/// to neutralize position bias).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct RankJudge {
    pub id: String,
    pub judge: Seat,
    /// Trial ids mapped to `A`, `B`, `C`, ... for this pass.
    pub order: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    /// Ranked tiers of trial ids from best to worst; elements in the same inner Vec tied.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ranking: Vec<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub done: bool,
}

impl RankJudge {
    /// Returns the unique top-ranked trial id when tier 0 has a single winner (no tie at #1).
    pub fn top_trial(&self) -> Option<&str> {
        let first = self.ranking.first()?;
        (first.len() == 1).then(|| first[0].as_str())
    }

    /// Returns the 0-based tier index of `trial_id` in `ranking` (lower is better).
    pub fn tier_of(&self, trial_id: &str) -> Option<usize> {
        self.ranking.iter().position(|tier| tier.iter().any(|id| id == trial_id))
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rankings: Vec<RankJudge>,
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

/// Checks `[experiments.<name>]` presets in `config.toml` without requiring any harness binary
/// on disk (ACP availability is checked at `exp run` time).
pub fn check_presets(cfg: &Config) -> std::result::Result<(), String> {
    for (name, preset) in &cfg.experiments {
        if name.trim().is_empty() {
            return Err("[experiments] has a preset with an empty name.".into());
        }
        if preset.arms.len() < 2 {
            return Err(format!("[experiments.{name}] needs at least two entries in `arms`."));
        }
        if let Some(0) = preset.max_parallel {
            return Err(format!("[experiments.{name}] max_parallel must be at least 1."));
        }
        let mut accounts: Vec<String> = Vec::new();
        for raw in &preset.arms {
            let arm = parse_arm(cfg, raw).map_err(|e| format!("[experiments.{name}] {e}"))?;
            accounts.push(arm.worker.account);
            if let Some(rev) = arm.reviewer {
                accounts.push(rev.account);
            }
        }
        let has_single = preset.judge.as_deref().map(str::trim).is_some_and(|s| !s.is_empty());
        if has_single && !preset.judges.is_empty() {
            return Err(format!("[experiments.{name}] sets both `judge` and `judges`; use one or the other."));
        }
        for raw_judge in preset.resolved_judges() {
            let seat = parse_seat(cfg, &raw_judge).map_err(|e| format!("[experiments.{name}] judge: {e}"))?;
            accounts.push(seat.account);
        }
        let refs: Vec<&str> = accounts.iter().map(String::as_str).collect();
        check_boundary(cfg, &refs).map_err(|e| format!("[experiments.{name}] {e}"))?;
    }
    Ok(())
}

/// Validates every seat in a run (ACP hosted support, boundary consistency, repo hooks) and returns
/// non-fatal warnings (such as a single judge sharing a harness with an arm).
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
        let seats = j.resolved_seats();
        let mut judge_harnesses = HashSet::new();
        for seat in &seats {
            let acct = ensure_acp_seat(cfg, seat, &user_home)?;
            all_accounts.push(&acct.name);
            judge_harnesses.insert((acct.name.as_str(), acct.harness.as_str()));
        }
        if seats.len() == 1 {
            if let Some(&(name, harness)) = judge_harnesses.iter().next() {
                if arm_harnesses.contains(harness) {
                    warnings.push(format!("Judge account {name} uses the same harness ({harness}) as an arm; consider an out-of-family judge or a multi-provider judge panel to reduce self-preference bias."));
                }
            }
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

/// Builds pairwise comparisons for every unordered pair of surviving trials across all judge seats.
pub fn build_comparisons(trials: &[Trial], judges: &[&Seat]) -> Vec<PairComparison> {
    let survivors: Vec<&str> = trials.iter().filter(|t| t.survives()).map(|t| t.id.as_str()).collect();
    let seats: Vec<Option<Seat>> = if judges.is_empty() { vec![None] } else { judges.iter().map(|s| Some((*s).clone())).collect() };
    let mut out = Vec::new();
    for i in 0..survivors.len() {
        for j in (i + 1)..survivors.len() {
            for seat in &seats {
                out.push(PairComparison::new(survivors[i].to_string(), survivors[j].to_string(), seat.clone()));
            }
        }
    }
    out
}

/// Builds blind all-at-once ranking passes over all surviving trials.
///
/// - For 2+ judges: one pass per judge, cyclically rotating candidate labels (`A,B,C`, `B,C,A`,
///   `C,A,B`) so every arm occupies every position across the panel.
/// - For 1 judge: two passes (forward and reversed) so position bias is still checked.
pub fn build_rankings(trials: &[Trial], judges: &[&Seat]) -> Vec<RankJudge> {
    let survivors: Vec<String> = trials.iter().filter(|t| t.survives()).map(|t| t.id.clone()).collect();
    if survivors.len() < 2 || judges.is_empty() {
        return Vec::new();
    }
    if judges.len() == 1 {
        let mut rev = survivors.clone();
        rev.reverse();
        return vec![
            RankJudge { id: "j0-fwd".into(), judge: judges[0].clone(), order: survivors, ..Default::default() },
            RankJudge { id: "j0-rev".into(), judge: judges[0].clone(), order: rev, ..Default::default() },
        ];
    }
    let n = survivors.len();
    judges
        .iter()
        .enumerate()
        .map(|(idx, seat)| {
            let mut order = survivors.clone();
            order.rotate_left(idx % n);
            RankJudge { id: format!("j{idx}"), judge: (*seat).clone(), order, ..Default::default() }
        })
        .collect()
}

/// Counts head-to-head ballot wins (`a_ahead`, `b_ahead`) between two trials across all completed
/// ranking ballots that include both trials.
pub(crate) fn rank_head_to_head(rankings: &[RankJudge], a: &str, b: &str) -> (usize, usize) {
    let mut a_ahead = 0;
    let mut b_ahead = 0;
    for rj in rankings {
        if let (Some(ta), Some(tb)) = (rj.tier_of(a), rj.tier_of(b)) {
            if ta < tb {
                a_ahead += 1;
            } else if tb < ta {
                b_ahead += 1;
            }
        }
    }
    (a_ahead, b_ahead)
}

/// Computes the overall recommendation from surviving trials and either all-at-once `rankings`
/// (Borda score + first-place votes) or pairwise `comparisons` (consistent forward+reversed votes).
pub fn compute_recommendation(trials: &[Trial], comparisons: &[PairComparison], rankings: &[RankJudge]) -> (Option<String>, String) {
    let survivors: Vec<&Trial> = trials.iter().filter(|t| t.survives()).collect();
    match survivors.len() {
        0 => return (None, "No arm produced a passing diff.".into()),
        1 => return (Some(survivors[0].id.clone()), format!("Only surviving arm ({}).", survivors[0].arm.label)),
        _ => {}
    }
    if !rankings.is_empty() {
        return compute_rank_recommendation(&survivors, rankings);
    }
    if comparisons.is_empty() {
        return (None, "No judge configured; compare surviving diffs by hand.".into());
    }
    // Group pairwise comparisons by unordered pair so multi-judge pairwise panels vote per pair.
    let mut pair_votes: BTreeMap<(&str, &str), (usize, usize)> = BTreeMap::new();
    for c in comparisons {
        let (a, b) = if c.trial_a <= c.trial_b { (c.trial_a.as_str(), c.trial_b.as_str()) } else { (c.trial_b.as_str(), c.trial_a.as_str()) };
        let entry = pair_votes.entry((a, b)).or_insert((0, 0));
        if c.consistent_winner.as_deref() == Some(a) {
            entry.0 += 1;
        } else if c.consistent_winner.as_deref() == Some(b) {
            entry.1 += 1;
        }
    }
    let mut wins: BTreeMap<&str, usize> = survivors.iter().map(|t| (t.id.as_str(), 0)).collect();
    let mut pair_winner: BTreeMap<(&str, &str), Option<&str>> = BTreeMap::new();
    for ((a, b), (va, vb)) in &pair_votes {
        let w = if va > vb {
            if let Some(slot) = wins.get_mut(*a) {
                *slot += 1;
            }
            Some(*a)
        } else if vb > va {
            if let Some(slot) = wins.get_mut(*b) {
                *slot += 1;
            }
            Some(*b)
        } else {
            None
        };
        pair_winner.insert((*a, *b), w);
    }
    let mut ranked: Vec<(&str, usize)> = wins.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let (top_id, top_wins) = ranked[0];
    let (second_id, second_wins) = ranked[1];
    if top_wins == 0 || top_wins == second_wins {
        return (None, "Too close: pairwise orderings did not agree on a clear winner.".into());
    }
    let pair_key = if top_id <= second_id { (top_id, second_id) } else { (second_id, top_id) };
    if pair_winner.get(&pair_key).copied().flatten() != Some(top_id) {
        return (None, "Too close: top arms did not have a consistent head-to-head winner.".into());
    }
    let label = survivors.iter().find(|t| t.id == top_id).map_or(top_id, |t| t.arm.label.as_str());
    (Some(top_id.to_string()), format!("Recommended {label} ({top_wins}/{} consistent pairwise wins).", survivors.len() - 1))
}

fn compute_rank_recommendation(survivors: &[&Trial], rankings: &[RankJudge]) -> (Option<String>, String) {
    let valid: Vec<&RankJudge> = rankings.iter().filter(|r| !r.ranking.is_empty()).collect();
    if valid.is_empty() {
        return (None, "No judge produced a valid ranking.".into());
    }
    // Single-judge forward/reversed mode requires both orders to agree on #1.
    let single_judge_pair = rankings.len() == 2 && rankings[0].id.ends_with("-fwd") && rankings[1].id.ends_with("-rev");
    if single_judge_pair {
        if valid.len() < 2 {
            return (None, "Too close: single judge did not complete both forward and reversed rankings.".into());
        }
        let fwd_top = rankings[0].top_trial();
        let rev_top = rankings[1].top_trial();
        return match (fwd_top, rev_top) {
            (Some(f), Some(r)) if f == r => {
                let label = survivors.iter().find(|t| t.id == f).map_or(f, |t| t.arm.label.as_str());
                (Some(f.to_string()), format!("Recommended {label} (#1 in both forward and reversed rankings)."))
            }
            _ => (None, "Too close: forward and reversed ranking passes did not agree on #1.".into()),
        };
    }

    let n = survivors.len();
    let mut borda: BTreeMap<&str, f64> = survivors.iter().map(|t| (t.id.as_str(), 0.0)).collect();
    let mut firsts: BTreeMap<&str, usize> = survivors.iter().map(|t| (t.id.as_str(), 0)).collect();
    for rj in &valid {
        let mut pos = 0usize;
        for tier in &rj.ranking {
            if tier.is_empty() {
                continue;
            }
            // Standard tie-split Borda points: average of (n - 1 - k) for k in pos..(pos + tier.len()).
            let sum_pts: f64 = (pos..(pos + tier.len())).map(|k| n.saturating_sub(1 + k) as f64).sum();
            let avg_pts = sum_pts / (tier.len() as f64);
            for id in tier {
                if let Some(slot) = borda.get_mut(id.as_str()) {
                    *slot += avg_pts;
                }
            }
            pos += tier.len();
        }
        if let Some(top) = rj.top_trial() {
            if let Some(slot) = firsts.get_mut(top) {
                *slot += 1;
            }
        }
    }

    let mut ranked: Vec<(&str, f64, usize)> = survivors
        .iter()
        .map(|t| (t.id.as_str(), borda[t.id.as_str()], firsts[t.id.as_str()]))
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| b.2.cmp(&a.2)).then_with(|| a.0.cmp(b.0)));

    let (top_id, top_pts, top_firsts) = ranked[0];
    let (_, second_pts, _) = ranked[1];
    if top_pts <= second_pts + 1e-9 {
        return (None, format!("Too close: tied at the top ({top_pts:.1} Borda pts across {} judges).", valid.len()));
    }
    let max_pts = ((n - 1) * valid.len()) as f64;
    let label = survivors.iter().find(|t| t.id == top_id).map_or(top_id, |t| t.arm.label.as_str());
    (
        Some(top_id.to_string()),
        format!("Recommended {label} ({top_pts:.1}/{max_pts:.1} Borda pts, {top_firsts}/{} #1 votes across {} judges).", valid.len(), valid.len()),
    )
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
        rankings: Vec::new(),
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
        let mut c = PairComparison::new("t1".into(), "t2".into(), None);
        c.forward.done = true;
        c.forward.winner = Some("A".into());
        c.reversed.done = true;
        c.reversed.winner = Some("B".into());
        c.resolve();
        assert_eq!(c.consistent_winner.as_deref(), Some("t1"));
        let (rec, _) = compute_recommendation(&trials, &[c], &[]);
        assert_eq!(rec.as_deref(), Some("t1"));

        // Order bias (both pick "A") -> inconsistent -> too close.
        let mut biased = PairComparison::new("t1".into(), "t2".into(), None);
        biased.forward.done = true;
        biased.forward.winner = Some("A".into());
        biased.reversed.done = true;
        biased.reversed.winner = Some("A".into());
        biased.resolve();
        assert_eq!(biased.consistent_winner, None);
        let (rec, note) = compute_recommendation(&trials, &[biased], &[]);
        assert_eq!(rec, None);
        assert!(note.contains("Too close"), "{note}");
    }

    #[test]
    fn rankings_rotate_cyclically_and_score_borda_with_tie_splitting() {
        let trial = |id: &str| Trial { id: id.into(), arm: Arm { label: id.into(), ..Default::default() }, stage: TrialStage::Ready, diff_bytes: 100, ..Default::default() };
        let trials = vec![trial("t1"), trial("t2"), trial("t3")];
        let j1 = Seat { account: "claude".into(), ..Default::default() };
        let j2 = Seat { account: "codex".into(), ..Default::default() };
        let j3 = Seat { account: "antigravity".into(), ..Default::default() };
        let mut panel = build_rankings(&trials, &[&j1, &j2, &j3]);
        assert_eq!(panel.len(), 3);
        assert_eq!(panel[0].order, vec!["t1", "t2", "t3"]);
        assert_eq!(panel[1].order, vec!["t2", "t3", "t1"]);
        assert_eq!(panel[2].order, vec!["t3", "t1", "t2"]);

        // j1: t2 > t1 > t3 (t2: 2, t1: 1, t3: 0)
        // j2: t2 > t3 = t1 (t2: 2, t3: 0.5, t1: 0.5)
        // j3: t1 > t2 > t3 (t1: 2, t2: 1, t3: 0)
        // Totals: t2 = 5.0 (2 #1 votes), t1 = 3.5 (1 #1 vote), t3 = 0.5.
        panel[0].ranking = vec![vec!["t2".into()], vec!["t1".into()], vec!["t3".into()]];
        panel[1].ranking = vec![vec!["t2".into()], vec!["t3".into(), "t1".into()]];
        panel[2].ranking = vec![vec!["t1".into()], vec!["t2".into()], vec!["t3".into()]];
        for r in &mut panel {
            r.done = true;
        }
        let (rec, note) = compute_recommendation(&trials, &[], &panel);
        assert_eq!(rec.as_deref(), Some("t2"));
        assert!(note.contains("5.0/6.0 Borda pts, 2/3 #1 votes"), "{note}");

        // Single judge builds fwd + rev passes and requires agreement on #1.
        let mut single = build_rankings(&trials, &[&j1]);
        assert_eq!(single.len(), 2);
        assert_eq!(single[0].order, vec!["t1", "t2", "t3"]);
        assert_eq!(single[1].order, vec!["t3", "t2", "t1"]);
        single[0].ranking = vec![vec!["t2".into()], vec!["t1".into()], vec!["t3".into()]];
        single[1].ranking = vec![vec!["t1".into()], vec!["t2".into()], vec!["t3".into()]];
        let (rec_disagree, note_disagree) = compute_recommendation(&trials, &[], &single);
        assert_eq!(rec_disagree, None);
        assert!(note_disagree.contains("did not agree"), "{note_disagree}");
    }

    #[test]
    fn check_presets_validates_arms_judge_boundaries_and_allows_no_eval() {
        let mut cfg = test_cfg();
        cfg.experiments.insert(
            "quality".into(),
            PresetCfg {
                arms: vec!["claude:opus:high".into(), "claude:sonnet+codex:medium".into()],
                judges: vec!["claude:opus:high".into(), "codex:high".into()],
                judge_mode: Some(JudgeMode::Rank),
                rubric: Some("Prefer idiomatic reuse of existing helpers.".into()),
                eval: None,
                ..Default::default()
            },
        );
        assert!(check_presets(&cfg).is_ok());

        // Setting both `judge` and `judges` is rejected.
        cfg.experiments.insert(
            "both".into(),
            PresetCfg { arms: vec!["claude".into(), "codex".into()], judge: Some("claude".into()), judges: vec!["codex".into()], ..Default::default() },
        );
        assert!(check_presets(&cfg).unwrap_err().contains("both `judge` and `judges`"));
        cfg.experiments.remove("both");

        // Too few arms is rejected.
        cfg.experiments.insert("one".into(), PresetCfg { arms: vec!["claude".into()], ..Default::default() });
        assert!(check_presets(&cfg).unwrap_err().contains("at least two"));
        cfg.experiments.remove("one");

        // Cross-boundary preset is rejected.
        cfg.experiments.insert(
            "leak".into(),
            PresetCfg { arms: vec!["claude-work".into(), "codex".into()], ..Default::default() },
        );
        assert!(check_presets(&cfg).unwrap_err().contains("same boundary"));
        cfg.experiments.remove("leak");

        // max_parallel = 0 is rejected.
        cfg.experiments.insert(
            "zero".into(),
            PresetCfg { arms: vec!["claude".into(), "codex".into()], max_parallel: Some(0), ..Default::default() },
        );
        assert!(check_presets(&cfg).unwrap_err().contains("max_parallel"));
    }
}
