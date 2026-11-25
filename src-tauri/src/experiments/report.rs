//! Reporting, on-demand ledger cost pricing, leaderboard aggregation and lifecycle actions
//! (adopt, discard, cancel) for experiment runs.
//!
//! Session costs are priced on demand from `ledger::session_in` so no duplicate SQLite tables
//! are written and nothing contends with the ledger's catch-up thread.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{anyhow, bail, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::Config;
use crate::engine::Handle;
use crate::ledger::{self, SessionSpent};
use crate::util::now_ms;
use crate::worktree;

use super::{folder, gitops, runner, Op, Run, RunStage, Trial, TrialStage};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CostBreakdown {
    pub total_api: f64,
    pub worker_api: f64,
    pub reviewer_api: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub unpriced_calls: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrialView {
    pub id: String,
    pub label: String,
    pub worker: String,
    pub reviewer: Option<String>,
    pub observed_model: Option<String>,
    pub stage: TrialStage,
    pub worktree: Option<String>,
    pub branch: Option<String>,
    pub worker_key: Option<String>,
    pub reviewer_key: Option<String>,
    pub diff_bytes: usize,
    pub diff_ref: Option<String>,
    pub review_verdict: Option<String>,
    pub reviewer_modified_tree: bool,
    pub gate_passed: Option<bool>,
    pub pr_opened: bool,
    pub drift: Option<String>,
    pub error: Option<String>,
    pub interruptions: u32,
    pub duration_ms: Option<i64>,
    pub cost: CostBreakdown,
    pub adopted: bool,
    pub discarded: bool,
    pub recommended: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PairView {
    pub trial_a: String,
    pub trial_b: String,
    pub label_a: String,
    pub label_b: String,
    pub forward_winner: Option<String>,
    pub reversed_winner: Option<String>,
    pub consistent_winner: Option<String>,
    pub rationale: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunView {
    pub id: String,
    pub experiment: String,
    pub repo: String,
    pub base_sha: String,
    pub prompt: String,
    pub stage: RunStage,
    pub created_at: i64,
    pub finished_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub base_gate_ok: Option<bool>,
    pub recommended: Option<String>,
    pub recommendation_note: Option<String>,
    pub adopted_trial: Option<String>,
    pub judge_cost_api: f64,
    pub error: Option<String>,
    pub trials: Vec<TrialView>,
    pub comparisons: Vec<PairView>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LeaderboardRow {
    pub key: String,
    pub experiment: String,
    pub worker_harness: String,
    pub worker_model: String,
    pub worker_effort: Option<String>,
    pub reviewer: Option<String>,
    pub label: String,
    pub runs: u32,
    pub wins: u32,
    pub ties: u32,
    pub losses: u32,
    pub gate_passed: u32,
    pub gate_total: u32,
    pub avg_cost_api: f64,
    pub avg_worker_cost_api: f64,
    pub avg_reviewer_cost_api: f64,
    pub avg_duration_ms: Option<i64>,
    pub adopted_count: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub on: bool,
    pub experiments: Vec<String>,
    pub leaderboard: Vec<LeaderboardRow>,
    pub runs: Vec<RunView>,
}

/// Builds the full Experiments report for the Analytics window and `codealong exp results`.
pub fn build(cfg: &Config, filter_experiment: Option<&str>) -> Result<Report> {
    if !cfg.experimental.experiments {
        return Ok(Report::default());
    }
    let file = super::load()?;
    ledger::with_db(|db| build_in(db, cfg, &file.runs, filter_experiment))
}

pub fn build_in(db: &Connection, cfg: &Config, all_runs: &[Run], filter_experiment: Option<&str>) -> Result<Report> {
    let experiments: Vec<String> = all_runs.iter().map(|r| r.experiment.clone()).collect::<BTreeSet<_>>().into_iter().collect();
    let mut runs: Vec<RunView> = Vec::new();
    let mut agg: BTreeMap<String, Agg> = BTreeMap::new();

    for run in all_runs.iter().rev() {
        if let Some(f) = filter_experiment.filter(|s| !s.is_empty()) {
            if run.experiment != f {
                continue;
            }
        }
        let rv = build_run_view(db, cfg, run)?;
        accumulate_leaderboard(&mut agg, cfg, run, &rv);
        runs.push(rv);
    }

    let mut leaderboard: Vec<LeaderboardRow> = agg.into_values().map(Agg::finish).collect();
    leaderboard.sort_by(|a, b| {
        let rate_a = win_rate(a);
        let rate_b = win_rate(b);
        rate_b.total_cmp(&rate_a).then_with(|| b.wins.cmp(&a.wins)).then_with(|| a.avg_cost_api.total_cmp(&b.avg_cost_api))
    });

    Ok(Report { on: true, experiments, leaderboard, runs })
}

fn win_rate(r: &LeaderboardRow) -> f64 {
    let decided = r.wins + r.losses + r.ties;
    if decided == 0 {
        0.0
    } else {
        (r.wins as f64 + 0.5 * r.ties as f64) / decided as f64
    }
}

fn price_session(db: &Connection, cfg: &Config, key: &str) -> SessionSpent {
    ledger::session_in(db, cfg, key).unwrap_or_default()
}

fn add_spent(c: &mut CostBreakdown, s: &SessionSpent, role: &str) {
    c.total_api += s.total.api;
    match role {
        "worker" => c.worker_api += s.total.api,
        "reviewer" => c.reviewer_api += s.total.api,
        _ => {}
    }
    c.input_tokens += s.total.tokens.input;
    c.output_tokens += s.total.tokens.output;
    c.cache_read_tokens += s.total.tokens.cache_read;
    c.unpriced_calls += s.total.unpriced;
}

fn build_run_view(db: &Connection, cfg: &Config, run: &Run) -> Result<RunView> {
    let mut trials = Vec::with_capacity(run.trials.len());
    for t in &run.trials {
        let mut cost = CostBreakdown::default();
        let mut observed_model: Option<String> = None;
        for ts in &t.sessions {
            let sp = price_session(db, cfg, &ts.key);
            add_spent(&mut cost, &sp, &ts.role);
            if ts.role == "worker" && observed_model.is_none() {
                observed_model = sp.models.first().map(|m| m.key.clone());
            }
        }
        let duration_ms = match (t.times.started_at, t.times.finished_at) {
            (Some(s), Some(e)) if e >= s => Some(e - s),
            _ => None,
        };
        let drift = t.drift.clone().or_else(|| check_model_drift(t.arm.worker.model.as_deref(), observed_model.as_deref()));
        trials.push(TrialView {
            id: t.id.clone(),
            label: t.arm.label.clone(),
            worker: t.arm.worker.short(),
            reviewer: t.arm.reviewer.as_ref().map(|r| r.short()),
            observed_model,
            stage: t.stage,
            worktree: t.worktree.clone(),
            branch: t.branch.clone(),
            worker_key: t.worker_key.clone(),
            reviewer_key: t.reviewer_key.clone(),
            diff_bytes: t.diff_bytes,
            diff_ref: t.diff_ref.clone(),
            review_verdict: t.review_verdict.clone(),
            reviewer_modified_tree: t.reviewer_modified_tree,
            gate_passed: t.gate_passed,
            pr_opened: t.pr_opened,
            drift,
            error: t.error.clone(),
            interruptions: t.interruptions,
            duration_ms,
            cost,
            adopted: t.adopted,
            discarded: t.discarded,
            recommended: run.recommended.as_deref() == Some(t.id.as_str()),
        });
    }

    let mut judge_cost_api = 0.0;
    let mut comparisons = Vec::with_capacity(run.comparisons.len());
    let label_of = |id: &str| run.trials.iter().find(|t| t.id == id).map_or_else(|| id.to_string(), |t| t.arm.label.clone());
    for c in &run.comparisons {
        for key in c.forward.session_key.iter().chain(c.reversed.session_key.iter()) {
            judge_cost_api += price_session(db, cfg, key).total.api;
        }
        comparisons.push(PairView {
            trial_a: c.trial_a.clone(),
            trial_b: c.trial_b.clone(),
            label_a: label_of(&c.trial_a),
            label_b: label_of(&c.trial_b),
            forward_winner: c.forward.winning_trial().map(str::to_string),
            reversed_winner: c.reversed.winning_trial().map(str::to_string),
            consistent_winner: c.consistent_winner.clone(),
            rationale: c.forward.rationale.clone().or_else(|| c.reversed.rationale.clone()),
        });
    }

    let duration_ms = run.finished_at.and_then(|e| (e >= run.created_at).then_some(e - run.created_at));
    Ok(RunView {
        id: run.id.clone(),
        experiment: run.experiment.clone(),
        repo: run.repo.clone(),
        base_sha: run.base_sha.clone(),
        prompt: run.prompt.clone(),
        stage: run.stage,
        created_at: run.created_at,
        finished_at: run.finished_at,
        duration_ms,
        base_gate_ok: run.base_gate_ok,
        recommended: run.recommended.clone(),
        recommendation_note: run.recommendation_note.clone(),
        adopted_trial: run.adopted_trial.clone(),
        judge_cost_api,
        error: run.error.clone(),
        trials,
        comparisons,
    })
}

/// Checks whether the observed primary model in the ledger plainly contradicts a pinned model
/// alias (e.g. pinned `opus` vs observed `claude-haiku-...`), while allowing family aliases like
/// `opus` matching `claude-opus-5-5` or `opus[1m]`.
fn check_model_drift(pinned: Option<&str>, observed: Option<&str>) -> Option<String> {
    let (Some(p), Some(o)) = (pinned, observed) else { return None };
    let base = p.split('[').next().unwrap_or(p).trim().to_ascii_lowercase();
    if base.is_empty() {
        return None;
    }
    let obs = o.to_ascii_lowercase();
    if obs.contains(&base) || base.contains(&obs) {
        None
    } else {
        Some(format!("Pinned {p}, observed {o}"))
    }
}

struct Agg {
    key: String,
    experiment: String,
    worker_harness: String,
    worker_model: String,
    worker_effort: Option<String>,
    reviewer: Option<String>,
    label: String,
    runs: u32,
    wins: u32,
    ties: u32,
    losses: u32,
    gate_passed: u32,
    gate_total: u32,
    cost_sum: f64,
    worker_cost_sum: f64,
    reviewer_cost_sum: f64,
    dur_sum: i64,
    dur_n: i64,
    adopted_count: u32,
}

impl Agg {
    fn finish(self) -> LeaderboardRow {
        let n = self.runs.max(1) as f64;
        LeaderboardRow {
            key: self.key,
            experiment: self.experiment,
            worker_harness: self.worker_harness,
            worker_model: self.worker_model,
            worker_effort: self.worker_effort,
            reviewer: self.reviewer,
            label: self.label,
            runs: self.runs,
            wins: self.wins,
            ties: self.ties,
            losses: self.losses,
            gate_passed: self.gate_passed,
            gate_total: self.gate_total,
            avg_cost_api: self.cost_sum / n,
            avg_worker_cost_api: self.worker_cost_sum / n,
            avg_reviewer_cost_api: self.reviewer_cost_sum / n,
            avg_duration_ms: (self.dur_n > 0).then(|| self.dur_sum / self.dur_n),
            adopted_count: self.adopted_count,
        }
    }
}

fn accumulate_leaderboard(agg: &mut BTreeMap<String, Agg>, cfg: &Config, run: &Run, rv: &RunView) {
    if run.stage != RunStage::Complete {
        return;
    }
    for (t, tv) in run.trials.iter().zip(rv.trials.iter()) {
        if t.stage == TrialStage::Voided {
            continue;
        }
        let harness = cfg.accounts.iter().find(|a| a.name == t.arm.worker.account).map_or_else(|| t.arm.worker.account.clone(), |a| a.harness.clone());
        let model = tv.observed_model.clone().or_else(|| t.arm.worker.model.clone()).unwrap_or_else(|| "default".into());
        let effort = t.arm.worker.effort.clone();
        let reviewer = t.arm.reviewer.as_ref().map(|r| {
            let rh = cfg.accounts.iter().find(|a| a.name == r.account).map_or_else(|| r.account.clone(), |a| a.harness.clone());
            match (&r.model, &r.effort) {
                (Some(m), Some(e)) => format!("{rh}/{m}/{e}"),
                (Some(m), None) => format!("{rh}/{m}"),
                (None, Some(e)) => format!("{rh}/{e}"),
                (None, None) => rh,
            }
        });
        let key = format!("{}|{}|{}|{}|{}", run.experiment, harness, model, effort.as_deref().unwrap_or(""), reviewer.as_deref().unwrap_or(""));
        let entry = agg.entry(key.clone()).or_insert_with(|| Agg {
            key,
            experiment: run.experiment.clone(),
            worker_harness: harness,
            worker_model: model,
            worker_effort: effort,
            reviewer,
            label: t.arm.label.clone(),
            runs: 0,
            wins: 0,
            ties: 0,
            losses: 0,
            gate_passed: 0,
            gate_total: 0,
            cost_sum: 0.0,
            worker_cost_sum: 0.0,
            reviewer_cost_sum: 0.0,
            dur_sum: 0,
            dur_n: 0,
            adopted_count: 0,
        });
        entry.runs += 1;
        if let Some(passed) = t.gate_passed {
            entry.gate_total += 1;
            if passed {
                entry.gate_passed += 1;
            }
        }
        entry.cost_sum += tv.cost.total_api;
        entry.worker_cost_sum += tv.cost.worker_api;
        entry.reviewer_cost_sum += tv.cost.reviewer_api;
        if let Some(ms) = tv.duration_ms {
            entry.dur_sum += ms;
            entry.dur_n += 1;
        }
        if t.adopted {
            entry.adopted_count += 1;
        }
        // Tally pairwise comparison results for this trial.
        for c in &run.comparisons {
            if c.trial_a != t.id && c.trial_b != t.id {
                continue;
            }
            if !c.is_done() {
                continue;
            }
            match c.consistent_winner.as_deref() {
                Some(w) if w == t.id => entry.wins += 1,
                Some(_) => entry.losses += 1,
                None => entry.ties += 1,
            }
        }
    }
}

/// Executes Adopt / Discard / Cancel on `experiments.json` and active sessions/worktrees.
pub fn apply_op(engine: Option<&Handle>, op: Op) -> Result<Value> {
    match op {
        Op::Adopt { run_id, trial_id } => {
            let mut adopted_wt: Option<String> = None;
            let mut agreed: Option<bool> = None;
            super::change(|f| {
                let Some(run) = f.runs.iter_mut().find(|r| r.id == run_id) else { return false };
                let Some(t) = run.trials.iter_mut().find(|t| t.id == trial_id) else { return false };
                if t.discarded {
                    return false;
                }
                for other in &mut run.trials {
                    other.adopted = other.id == trial_id;
                }
                run.adopted_trial = Some(trial_id.clone());
                agreed = run.recommended.as_deref().map(|r| r == trial_id);
                adopted_wt = run.trials.iter().find(|t| t.id == trial_id).and_then(|t| t.worktree.clone());
                true
            })?;
            let Some(wt) = adopted_wt else { bail!("Trial {trial_id} in run {run_id} couldn't be adopted (missing or discarded).") };
            Ok(serde_json::json!({ "runId": run_id, "trialId": trial_id, "worktree": wt, "agreedWithJudge": agreed }))
        }
        Op::Discard { run_id, trial_id } => {
            let file = super::load()?;
            let run = file.runs.iter().find(|r| r.id == run_id).ok_or_else(|| anyhow!("No experiment run {run_id}."))?;
            let targets: Vec<&Trial> = match trial_id.as_deref() {
                Some(tid) => run.trials.iter().filter(|t| t.id == tid).collect(),
                None => run.trials.iter().filter(|t| !t.adopted && !t.discarded).collect(),
            };
            if targets.is_empty() {
                bail!("No matching trials to discard in {run_id}.");
            }
            let mut removed = Vec::new();
            for t in &targets {
                // Save snapshot & patch BEFORE removing worktree so nothing is ever lost.
                if let Some(wt) = t.worktree.as_deref() {
                    let wt_path = Path::new(wt);
                    if wt_path.exists() {
                        if let Ok(snap) = gitops::snapshot(wt_path, &run.base_sha, Some((&run.id, &t.id))) {
                            let tdir = folder(&run.id).join(&t.id);
                            let _ = std::fs::create_dir_all(&tdir);
                            let _ = std::fs::write(tdir.join("diff.patch"), &snap.diff);
                        }
                    }
                    if let Some(eng) = engine {
                        for ts in &t.sessions {
                            runner::mark_done(eng, &ts.key);
                        }
                    }
                    worktree::force_remove(Path::new(&run.main_worktree), wt_path, t.branch.as_deref())?;
                }
                removed.push(t.id.clone());
            }
            super::change(|f| {
                let Some(r) = f.runs.iter_mut().find(|r| r.id == run_id) else { return false };
                for id in &removed {
                    if let Some(t) = r.trials.iter_mut().find(|t| t.id == *id) {
                        t.discarded = true;
                    }
                }
                true
            })?;
            Ok(serde_json::json!({ "runId": run_id, "discarded": removed }))
        }
        Op::Cancel { run_id } => {
            let file = super::load()?;
            let run = file.runs.iter().find(|r| r.id == run_id).ok_or_else(|| anyhow!("No experiment run {run_id}."))?;
            for t in &run.trials {
                for ts in &t.sessions {
                    let _ = crate::acp::cancel(&ts.key);
                }
            }
            for c in &run.comparisons {
                for key in c.forward.session_key.iter().chain(c.reversed.session_key.iter()) {
                    let _ = crate::acp::cancel(key);
                }
            }
            let now = now_ms();
            super::change(|f| {
                if let Some(r) = f.runs.iter_mut().find(|r| r.id == run_id) {
                    r.stage = RunStage::Cancelled;
                    r.finished_at = Some(now);
                    return true;
                }
                false
            })?;
            Ok(serde_json::json!({ "runId": run_id, "stage": "cancelled" }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AccountCfg;
    use crate::experiments::{Arm, PairComparison, PhaseTimes, Seat, TrialSession};

    #[test]
    fn model_drift_allows_family_aliases_and_flags_mismatches() {
        assert_eq!(check_model_drift(Some("opus"), Some("claude-opus-5-5")), None);
        assert_eq!(check_model_drift(Some("opus[1m]"), Some("claude-opus-5-5")), None);
        assert!(check_model_drift(Some("opus"), Some("claude-haiku-5-5")).is_some());
    }

    #[test]
    fn report_prices_worker_and_reviewer_separately_and_ranks_leaderboard() {
        let db = ledger::in_memory();
        let mut cfg = Config::default();
        cfg.experimental.experiments = true;
        cfg.accounts = vec![
            toml::from_str::<AccountCfg>("name = \"claude\"\nharness = \"claude\"\nhome = \"~/.claude\"\n").unwrap(),
            toml::from_str::<AccountCfg>("name = \"codex\"\nharness = \"codex\"\nhome = \"~/.codex\"\n").unwrap(),
        ];
        // Insert calls for worker and reviewer sessions.
        let s1 = ledger::session_row(&db, "claude:claude:w1", "claude", "/r").unwrap();
        let s2 = ledger::session_row(&db, "codex:codex:r1", "codex", "/r").unwrap();
        let now = now_ms();
        db.execute("INSERT INTO calls (at, session, model, input, output) VALUES (?1, ?2, 'claude-opus-5-5', 100000, 10000)", rusqlite::params![now, s1]).unwrap();
        db.execute("INSERT INTO calls (at, session, model, input, output) VALUES (?1, ?2, 'gpt-5', 50000, 5000)", rusqlite::params![now, s2]).unwrap();

        let mut cmp = PairComparison::new("t1".into(), "t2".into());
        cmp.forward.done = true;
        cmp.forward.winner = Some("A".into());
        cmp.reversed.done = true;
        cmp.reversed.winner = Some("B".into());
        cmp.resolve();

        let run = Run {
            id: "exp-1".into(),
            experiment: "bench".into(),
            repo: "codealong".into(),
            main_worktree: "/r".into(),
            base_sha: "abc".into(),
            prompt: "Test".into(),
            max_parallel: 2,
            stage: RunStage::Complete,
            recommended: Some("t1".into()),
            created_at: now - 10_000,
            finished_at: Some(now),
            trials: vec![
                Trial {
                    id: "t1".into(),
                    arm: Arm { label: "claude/opus + codex".into(), worker: Seat { account: "claude".into(), model: Some("opus".into()), ..Default::default() }, reviewer: Some(Seat { account: "codex".into(), ..Default::default() }), ..Default::default() },
                    stage: TrialStage::Ready,
                    worker_key: Some("claude:claude:w1".into()),
                    reviewer_key: Some("codex:codex:r1".into()),
                    sessions: vec![
                        TrialSession { key: "claude:claude:w1".into(), role: "worker".into(), step: 0, started_at: now - 9000 },
                        TrialSession { key: "codex:codex:r1".into(), role: "reviewer".into(), step: 1, started_at: now - 4000 },
                    ],
                    times: PhaseTimes { queued_at: now - 10_000, started_at: Some(now - 9000), finished_at: Some(now), ..Default::default() },
                    diff_bytes: 500,
                    gate_passed: Some(true),
                    ..Default::default()
                },
                Trial {
                    id: "t2".into(),
                    arm: Arm { label: "codex".into(), worker: Seat { account: "codex".into(), ..Default::default() }, ..Default::default() },
                    stage: TrialStage::Ready,
                    times: PhaseTimes { queued_at: now - 10_000, started_at: Some(now - 9000), finished_at: Some(now), ..Default::default() },
                    diff_bytes: 400,
                    gate_passed: Some(true),
                    ..Default::default()
                },
            ],
            comparisons: vec![cmp],
            ..Default::default()
        };

        let rep = build_in(&db, &cfg, &[run], None).unwrap();
        assert_eq!(rep.runs.len(), 1);
        assert_eq!(rep.runs[0].trials[0].observed_model.as_deref(), Some("claude-opus-5-5"));
        assert!(rep.runs[0].trials[0].cost.worker_api > 0.0);
        assert_eq!(rep.leaderboard.len(), 2);
        assert_eq!(rep.leaderboard[0].wins, 1);
        assert_eq!(rep.leaderboard[1].losses, 1);
    }
}
