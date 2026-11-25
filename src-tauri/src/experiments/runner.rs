//! Server-side experiment runner: ticks every few seconds, evaluates `experiments.json` against
//! the engine's live `Board` via a pure `plan()` function, and executes blocking git, session,
//! gate and judge jobs on worker threads.

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};

use crate::actions::{self, From};
use crate::autoreview;
use crate::config::{AccountCfg, Config};
use crate::engine::Handle;
use crate::github::Verdict;
use crate::model::{Board, Session, Status};
use crate::start::{Place, StartSpec, Started};
use crate::util::now_ms;
use crate::worktree;

use super::{build_comparisons, compute_recommendation, folder, gitops, prompts, PairComparison, Run, RunStage, Seat, StoreFile, Trial, TrialStage};

/// How often the experiment runner checks active runs.
const TICK: Duration = Duration::from_secs(5);
/// If a newly created worktree lane hasn't appeared on the board after this long, fail the trial.
const PREPARE_TIMEOUT_MS: i64 = 2 * 60_000;

/// What a single tick decides to execute off the lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    CheckBaseGate { run_id: String },
    PrepareWorktree { run_id: String, trial_id: String },
    StartWorker { run_id: String, trial_id: String, lane_id: String },
    FinishWorker { run_id: String, trial_id: String },
    FinishReview { run_id: String, trial_id: String },
    FinishFix { run_id: String, trial_id: String },
    RunGate { run_id: String, trial_id: String },
    StartJudge { run_id: String, pair_idx: usize, swapped: bool },
    FinishJudge { run_id: String, pair_idx: usize, swapped: bool },
}

impl Job {
    fn busy_key(&self) -> String {
        match self {
            Job::CheckBaseGate { run_id } => format!("base:{run_id}"),
            Job::PrepareWorktree { run_id, trial_id }
            | Job::StartWorker { run_id, trial_id, .. }
            | Job::FinishWorker { run_id, trial_id }
            | Job::FinishReview { run_id, trial_id }
            | Job::FinishFix { run_id, trial_id }
            | Job::RunGate { run_id, trial_id } => format!("trial:{run_id}:{trial_id}"),
            Job::StartJudge { run_id, pair_idx, swapped } | Job::FinishJudge { run_id, pair_idx, swapped } => {
                format!("judge:{run_id}:{pair_idx}:{}", if *swapped { "r" } else { "f" })
            }
        }
    }

    fn uses_gate(&self) -> bool {
        matches!(self, Job::CheckBaseGate { .. } | Job::RunGate { .. })
    }
}

/// Pure state updates applied synchronously before spawning jobs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    VoidTrial { run_id: String, trial_id: String, reason: String },
    FailTrial { run_id: String, trial_id: String, reason: String },
    FlagPr { run_id: String, trial_id: String },
    CountInterruption { run_id: String, trial_id: String },
    BeginJudging { run_id: String, comparisons: Vec<PairComparison> },
    CompleteRun { run_id: String, recommended: Option<String>, note: String, at: i64 },
}

/// Whether `s`'s turn ended at or after `at`, excluding cut-off turns, usage limits, errors,
/// permission waits and background commands still running.
pub(crate) fn turn_ended(s: &Session, at: i64) -> bool {
    let ended = match s.status {
        Status::Unread | Status::Idle | Status::Stopped => true,
        Status::Needs => s.asks,
        Status::Working | Status::Error => false,
    };
    ended && s.last_activity >= at && !s.cut_short() && s.background.is_none()
}

fn find_session<'a>(board: &'a Board, key: &str) -> Option<&'a Session> {
    board.lanes.iter().flat_map(|l| &l.sessions).find(|s| s.key == key)
}

fn session_has_pr(board: &Board, key: &str) -> bool {
    board.lanes.iter().any(|l| l.pr.is_some() && l.sessions.iter().any(|s| s.key == key))
        || board.prs.iter().any(|p| p.sessions.iter().any(|ps| ps.key == key))
}

fn find_lane_id(board: &Board, worktree: &str) -> Option<String> {
    let want = Path::new(worktree);
    let canon = dunce::canonicalize(want).ok();
    board.lanes.iter().find(|l| {
        !l.starts_chats
            && l.device.is_none()
            && (l.path == worktree || canon.as_deref().is_some_and(|c| Path::new(&l.path) == c || dunce::canonicalize(&l.path).ok().as_deref() == Some(c)))
    }).map(|l| l.id.clone())
}

/// Pure scheduler step over `runs` and `board`, tested directly on made-up clocks.
pub fn plan(runs: &[Run], board: &Board, busy: &HashSet<String>, gate_busy: bool, now: i64) -> (Vec<Job>, Vec<Note>) {
    let mut jobs = Vec::new();
    let mut notes = Vec::new();
    let mut gate_taken = gate_busy;

    for run in runs {
        match run.stage {
            RunStage::Complete | RunStage::Cancelled | RunStage::Unknown => continue,
            RunStage::Running => {
                // If a gate requires a base check first, do that before launching any arm.
                if let Some(gate) = &run.gate {
                    if gate.check_base && run.base_gate_ok.is_none() {
                        let key = format!("base:{}", run.id);
                        if !gate_taken && !busy.contains(&key) {
                            jobs.push(Job::CheckBaseGate { run_id: run.id.clone() });
                            gate_taken = true;
                        }
                        continue;
                    }
                }

                // 1. Inspect active trials first (turn completion, limits, cut-off, lane readiness).
                let mut active = 0usize;
                let mut all_terminal = true;
                for t in &run.trials {
                    if t.stage.is_active() {
                        active += 1;
                    }
                    if !t.stage.is_terminal() {
                        all_terminal = false;
                    }
                    let tkey = format!("trial:{}:{}", run.id, t.id);
                    if busy.contains(&tkey) {
                        continue;
                    }
                    match t.stage {
                        TrialStage::Preparing => {
                            if let Some(wt) = t.worktree.as_deref() {
                                if let Some(lane_id) = find_lane_id(board, wt) {
                                    jobs.push(Job::StartWorker { run_id: run.id.clone(), trial_id: t.id.clone(), lane_id });
                                } else if t.times.started_at.is_some_and(|s| now - s > PREPARE_TIMEOUT_MS) {
                                    notes.push(Note::FailTrial { run_id: run.id.clone(), trial_id: t.id.clone(), reason: "Worktree lane never appeared on the board.".into() });
                                }
                            }
                        }
                        TrialStage::Working | TrialStage::Fixing => {
                            if let (Some(key), Some(sent)) = (t.worker_key.as_deref(), t.turn_sent_at) {
                                if let Some(s) = find_session(board, key) {
                                    if session_has_pr(board, key) && !t.pr_opened {
                                        notes.push(Note::FlagPr { run_id: run.id.clone(), trial_id: t.id.clone() });
                                    }
                                    if s.limit.is_some() {
                                        notes.push(Note::VoidTrial { run_id: run.id.clone(), trial_id: t.id.clone(), reason: "Worker hit a usage limit.".into() });
                                    } else if s.cut_off.is_some() {
                                        // Leave carry-on to reconnect.rs; never treat cut_off as ended.
                                        if t.interruptions == 0 {
                                            notes.push(Note::CountInterruption { run_id: run.id.clone(), trial_id: t.id.clone() });
                                        }
                                    } else if turn_ended(s, sent) {
                                        let job = if t.stage == TrialStage::Working {
                                            Job::FinishWorker { run_id: run.id.clone(), trial_id: t.id.clone() }
                                        } else {
                                            Job::FinishFix { run_id: run.id.clone(), trial_id: t.id.clone() }
                                        };
                                        jobs.push(job);
                                    }
                                }
                            }
                        }
                        TrialStage::Reviewing => {
                            if let (Some(key), Some(sent)) = (t.reviewer_key.as_deref(), t.turn_sent_at) {
                                if let Some(s) = find_session(board, key) {
                                    if s.limit.is_some() {
                                        notes.push(Note::VoidTrial { run_id: run.id.clone(), trial_id: t.id.clone(), reason: "Reviewer hit a usage limit.".into() });
                                    } else if s.cut_off.is_some() {
                                        if t.interruptions == 0 {
                                            notes.push(Note::CountInterruption { run_id: run.id.clone(), trial_id: t.id.clone() });
                                        }
                                    } else if turn_ended(s, sent) {
                                        jobs.push(Job::FinishReview { run_id: run.id.clone(), trial_id: t.id.clone() });
                                    }
                                }
                            }
                        }
                        TrialStage::Gating => {
                            if !gate_taken {
                                jobs.push(Job::RunGate { run_id: run.id.clone(), trial_id: t.id.clone() });
                                gate_taken = true;
                            }
                        }
                        _ => {}
                    }
                }

                // 2. Start queued trials up to `max_parallel`.
                let cap = run.max_parallel.max(1);
                for t in &run.trials {
                    if active >= cap {
                        break;
                    }
                    if t.stage != TrialStage::Queued {
                        continue;
                    }
                    let tkey = format!("trial:{}:{}", run.id, t.id);
                    if busy.contains(&tkey) {
                        continue;
                    }
                    jobs.push(Job::PrepareWorktree { run_id: run.id.clone(), trial_id: t.id.clone() });
                    active += 1;
                }

                // 3. Once every trial is terminal, move to Judging or Complete.
                if all_terminal {
                    let survivors = run.trials.iter().filter(|t| t.survives()).count();
                    if run.judge.is_some() && survivors >= 2 {
                        notes.push(Note::BeginJudging { run_id: run.id.clone(), comparisons: build_comparisons(&run.trials) });
                    } else {
                        let (rec, note) = compute_recommendation(&run.trials, &[]);
                        notes.push(Note::CompleteRun { run_id: run.id.clone(), recommended: rec, note, at: now });
                    }
                }
            }
            RunStage::Judging => {
                if run.comparisons.iter().all(PairComparison::is_done) {
                    let (rec, note) = compute_recommendation(&run.trials, &run.comparisons);
                    notes.push(Note::CompleteRun { run_id: run.id.clone(), recommended: rec, note, at: now });
                    continue;
                }
                // Run at most one judge order at a time per run so judge accounts aren't contended.
                let mut judge_active = false;
                for (idx, c) in run.comparisons.iter().enumerate() {
                    for (swapped, ord) in [(false, &c.forward), (true, &c.reversed)] {
                        if ord.done {
                            continue;
                        }
                        let jkey = format!("judge:{}:{idx}:{}", run.id, if swapped { "r" } else { "f" });
                        if busy.contains(&jkey) {
                            judge_active = true;
                            continue;
                        }
                        if let (Some(key), Some(started)) = (ord.session_key.as_deref(), ord.started_at) {
                            judge_active = true;
                            if let Some(s) = find_session(board, key) {
                                if turn_ended(s, started) {
                                    jobs.push(Job::FinishJudge { run_id: run.id.clone(), pair_idx: idx, swapped });
                                }
                            }
                        } else if !judge_active {
                            jobs.push(Job::StartJudge { run_id: run.id.clone(), pair_idx: idx, swapped });
                            judge_active = true;
                        }
                    }
                }
            }
        }
    }
    (jobs, notes)
}

/// Applies pure `Note`s to `StoreFile`.
pub fn apply_notes(file: &mut StoreFile, notes: &[Note]) -> bool {
    let mut changed = false;
    for note in notes {
        match note {
            Note::VoidTrial { run_id, trial_id, reason } => {
                if let Some(t) = find_trial_mut(file, run_id, trial_id) {
                    t.stage = TrialStage::Voided;
                    t.error = Some(reason.clone());
                    t.times.finished_at = Some(now_ms());
                    changed = true;
                }
            }
            Note::FailTrial { run_id, trial_id, reason } => {
                if let Some(t) = find_trial_mut(file, run_id, trial_id) {
                    t.stage = TrialStage::Failed;
                    t.error = Some(reason.clone());
                    t.times.finished_at = Some(now_ms());
                    changed = true;
                }
            }
            Note::FlagPr { run_id, trial_id } => {
                if let Some(t) = find_trial_mut(file, run_id, trial_id) {
                    if !t.pr_opened {
                        t.pr_opened = true;
                        changed = true;
                    }
                }
            }
            Note::CountInterruption { run_id, trial_id } => {
                if let Some(t) = find_trial_mut(file, run_id, trial_id) {
                    t.interruptions += 1;
                    changed = true;
                }
            }
            Note::BeginJudging { run_id, comparisons } => {
                if let Some(r) = file.runs.iter_mut().find(|r| r.id == *run_id) {
                    r.stage = RunStage::Judging;
                    r.comparisons = comparisons.clone();
                    changed = true;
                }
            }
            Note::CompleteRun { run_id, recommended, note, at } => {
                if let Some(r) = file.runs.iter_mut().find(|r| r.id == *run_id) {
                    r.stage = RunStage::Complete;
                    r.recommended = recommended.clone();
                    r.recommendation_note = Some(note.clone());
                    r.finished_at = Some(*at);
                    changed = true;
                }
            }
        }
    }
    changed
}

fn find_trial_mut<'a>(file: &'a mut StoreFile, run_id: &str, trial_id: &str) -> Option<&'a mut Trial> {
    file.runs.iter_mut().find(|r| r.id == run_id)?.trials.iter_mut().find(|t| t.id == trial_id)
}

#[derive(Default)]
struct InFlight {
    keys: HashSet<String>,
    gate_busy: bool,
}

/// Spawns the server-side experiment runner loop (`serve.rs`).
pub fn spawn(engine: Handle) {
    let inflight = Arc::new(Mutex::new(InFlight::default()));
    std::thread::spawn(move || loop {
        std::thread::sleep(TICK);
        let cfg = match crate::config::current() {
            Ok(c) if c.experimental.experiments => c,
            _ => continue,
        };
        let file = match super::load() {
            Ok(f) => f,
            Err(_) => continue,
        };
        if !file.runs.iter().any(|r| matches!(r.stage, RunStage::Running | RunStage::Judging)) {
            continue;
        }
        let board = engine.board();
        let (jobs, notes) = {
            let guard = inflight.lock().unwrap_or_else(|e| e.into_inner());
            plan(&file.runs, &board, &guard.keys, guard.gate_busy, now_ms())
        };
        if !notes.is_empty() {
            let _ = super::change(|f| apply_notes(f, &notes));
        }
        for job in jobs {
            let bkey = job.busy_key();
            let uses_gate = job.uses_gate();
            {
                let mut guard = inflight.lock().unwrap_or_else(|e| e.into_inner());
                if guard.keys.contains(&bkey) || (uses_gate && guard.gate_busy) {
                    continue;
                }
                guard.keys.insert(bkey.clone());
                if uses_gate {
                    guard.gate_busy = true;
                }
            }
            let eng = engine.clone();
            let cfg_c = cfg.clone();
            let inf = inflight.clone();
            std::thread::spawn(move || {
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_job(&eng, &cfg_c, &job)));
                if let Err(e) = res {
                    eprintln!("codealong: experiment job {bkey} panicked: {e:?}");
                }
                let mut guard = inf.lock().unwrap_or_else(|e| e.into_inner());
                guard.keys.remove(&bkey);
                if uses_gate {
                    guard.gate_busy = false;
                }
            });
        }
    });
}

fn run_job(engine: &Handle, cfg: &Config, job: &Job) {
    let res = match job {
        Job::CheckBaseGate { run_id } => exec_base_gate(cfg, run_id),
        Job::PrepareWorktree { run_id, trial_id } => exec_prepare_worktree(cfg, run_id, trial_id),
        Job::StartWorker { run_id, trial_id, lane_id } => exec_start_worker(engine, cfg, run_id, trial_id, lane_id),
        Job::FinishWorker { run_id, trial_id } => exec_finish_worker(engine, cfg, run_id, trial_id),
        Job::FinishReview { run_id, trial_id } => exec_finish_review(engine, cfg, run_id, trial_id),
        Job::FinishFix { run_id, trial_id } => exec_finish_fix(run_id, trial_id),
        Job::RunGate { run_id, trial_id } => exec_run_gate(run_id, trial_id),
        Job::StartJudge { run_id, pair_idx, swapped } => exec_start_judge(engine, cfg, run_id, *pair_idx, *swapped),
        Job::FinishJudge { run_id, pair_idx, swapped } => exec_finish_judge(engine, cfg, run_id, *pair_idx, *swapped),
    };
    if let Err(e) = res {
        eprintln!("codealong: experiment job {:?} failed: {e:#}", job);
        match job {
            Job::PrepareWorktree { run_id, trial_id }
            | Job::StartWorker { run_id, trial_id, .. }
            | Job::FinishWorker { run_id, trial_id }
            | Job::FinishReview { run_id, trial_id }
            | Job::FinishFix { run_id, trial_id }
            | Job::RunGate { run_id, trial_id } => {
                let _ = super::change(|f| {
                    if let Some(t) = find_trial_mut(f, run_id, trial_id) {
                        t.stage = TrialStage::Failed;
                        t.error = Some(format!("{e:#}"));
                        t.times.finished_at = Some(now_ms());
                        return true;
                    }
                    false
                });
            }
            Job::CheckBaseGate { run_id } => {
                let _ = super::change(|f| {
                    if let Some(r) = f.runs.iter_mut().find(|r| r.id == *run_id) {
                        r.base_gate_ok = Some(false);
                        r.stage = RunStage::Complete;
                        r.error = Some(format!("Base gate failed before launching arms: {e:#}"));
                        r.finished_at = Some(now_ms());
                        return true;
                    }
                    false
                });
            }
            Job::StartJudge { run_id, pair_idx, swapped } | Job::FinishJudge { run_id, pair_idx, swapped } => {
                let _ = super::change(|f| {
                    if let Some(c) = f.runs.iter_mut().find(|r| r.id == *run_id).and_then(|r| r.comparisons.get_mut(*pair_idx)) {
                        let ord = if *swapped { &mut c.reversed } else { &mut c.forward };
                        ord.error = Some(format!("{e:#}"));
                        ord.done = true;
                        c.resolve();
                        return true;
                    }
                    false
                });
            }
        }
    }
}

fn exec_base_gate(cfg: &Config, run_id: &str) -> Result<()> {
    let file = super::load()?;
    let run = file.runs.iter().find(|r| r.id == run_id).ok_or_else(|| anyhow!("Run {run_id} not found."))?;
    let gate = run.gate.as_ref().ok_or_else(|| anyhow!("No gate configured."))?;
    let created = worktree::create_at(cfg, Path::new(&run.main_worktree), &run.base_sha)?;
    let gate_res = gitops::run_gate(&created.path, &gate.cmd, gate.timeout_secs);
    let _ = worktree::force_remove(&created.main, &created.path, created.branch.as_deref());
    let res = gate_res?;
    if !res.passed {
        bail!("gate `{}` exited {} on base {}: {}", gate.cmd, res.exit_code, &run.base_sha[..run.base_sha.len().min(8)], res.output.lines().last().unwrap_or("failed"));
    }
    super::change(|f| {
        if let Some(r) = f.runs.iter_mut().find(|r| r.id == run_id) {
            r.base_gate_ok = Some(true);
            return true;
        }
        false
    })
}

fn exec_prepare_worktree(cfg: &Config, run_id: &str, trial_id: &str) -> Result<()> {
    let file = super::load()?;
    let run = file.runs.iter().find(|r| r.id == run_id).ok_or_else(|| anyhow!("Run {run_id} not found."))?;
    let dir = folder(run_id);
    std::fs::create_dir_all(&dir)?;
    let spec_path = dir.join("spec.md");
    if !spec_path.exists() {
        std::fs::write(&spec_path, &run.prompt)?;
    }
    let created = worktree::create_at(cfg, Path::new(&run.main_worktree), &run.base_sha)?;
    worktree::remember(&created);
    let wt_s = created.path.to_string_lossy().into_owned();
    let now = now_ms();
    super::change(|f| {
        if let Some(t) = find_trial_mut(f, run_id, trial_id) {
            t.worktree = Some(wt_s);
            t.branch = created.branch;
            t.stage = TrialStage::Preparing;
            t.times.started_at = Some(now);
            return true;
        }
        false
    })
}

/// Starts a hosted ACP session in `lane_id` with optional `seat.mode`, sending `prompt` only after
/// switching mode when a mode is configured.
fn start_hosted_in_lane(engine: &Handle, cfg: &Config, lane_id: &str, seat: &Seat, prompt: &str, helper_of: Option<&str>) -> Result<String> {
    let acct = find_account(cfg, &seat.account)?;
    let initial_prompt = if seat.mode.is_some() { String::new() } else { prompt.to_string() };
    let spec = StartSpec {
        lane_id: lane_id.to_string(),
        account: seat.account.clone(),
        place: Place::Codealong,
        prompt: initial_prompt,
        model: seat.model.clone(),
        effort: seat.effort.clone(),
        worktree: false,
        todos: Vec::new(),
        from: helper_of.map(String::from),
        helper: helper_of.is_some(),
        report: false,
    };
    let params = codealong_peer::api::params::Start { spec };
    let started: Started = actions::local(engine, &From::Here, codealong_peer::api::START, lane_id, params)
        .and_then(actions::typed::<Started>)
        .map_err(|e| anyhow!(e))?;
    let key = started.key.ok_or_else(|| anyhow!("Hosted start on {} returned no session key.", seat.account))?;
    if let Some(mode) = seat.mode.as_deref() {
        crate::acp::switch_mode(acct, &key, mode)?;
        crate::acp::send(acct, &key, prompt)?;
    }
    engine.quiet(vec![key.clone()], true);
    Ok(key)
}

fn find_account<'a>(cfg: &'a Config, name: &str) -> Result<&'a AccountCfg> {
    cfg.accounts.iter().find(|a| a.name == name).ok_or_else(|| anyhow!("No account named {name:?}."))
}

fn exec_start_worker(engine: &Handle, cfg: &Config, run_id: &str, trial_id: &str, lane_id: &str) -> Result<()> {
    let file = super::load()?;
    let run = file.runs.iter().find(|r| r.id == run_id).ok_or_else(|| anyhow!("Run {run_id} not found."))?;
    let trial = run.trials.iter().find(|t| t.id == trial_id).ok_or_else(|| anyhow!("Trial {trial_id} not found."))?;
    let prompt = prompts::worker_prompt(&run.prompt);
    let now = now_ms();
    let key = start_hosted_in_lane(engine, cfg, lane_id, &trial.arm.worker, &prompt, None)?;
    super::change(|f| {
        if let Some(t) = find_trial_mut(f, run_id, trial_id) {
            t.worker_key = Some(key.clone());
            t.record_session(key, "worker", 0, now);
            t.turn_sent_at = Some(now);
            t.stage = TrialStage::Working;
            return true;
        }
        false
    })
}

fn exec_finish_worker(engine: &Handle, cfg: &Config, run_id: &str, trial_id: &str) -> Result<()> {
    let file = super::load()?;
    let run = file.runs.iter().find(|r| r.id == run_id).ok_or_else(|| anyhow!("Run {run_id} not found."))?;
    let trial = run.trials.iter().find(|t| t.id == trial_id).ok_or_else(|| anyhow!("Trial {trial_id} not found."))?;
    let wt = trial.worktree.as_deref().ok_or_else(|| anyhow!("Trial {trial_id} has no worktree."))?;
    let snap = gitops::snapshot(Path::new(wt), &run.base_sha, None)?;
    let now = now_ms();
    let tdir = folder(run_id).join(trial_id);
    std::fs::create_dir_all(&tdir)?;

    if let Some(rev) = &trial.arm.reviewer {
        if !snap.diff.trim().is_empty() {
            let spec_path = folder(run_id).join("spec.md");
            let diff_path = tdir.join("review-input.diff");
            std::fs::write(&diff_path, autoreview::numbered(&snap.diff))?;
            let board = engine.board();
            let lane_id = find_lane_id(&board, wt).unwrap_or_else(|| wt.to_string());
            let prompt = prompts::reviewer_prompt(&spec_path, &diff_path);
            let rev_key = start_hosted_in_lane(engine, cfg, &lane_id, rev, &prompt, trial.worker_key.as_deref())?;
            return super::change(|f| {
                if let Some(t) = find_trial_mut(f, run_id, trial_id) {
                    t.times.worker_done_at = Some(now);
                    t.tree_before_review = Some(snap.tree);
                    t.reviewer_key = Some(rev_key.clone());
                    t.record_session(rev_key, "reviewer", 1, now);
                    t.turn_sent_at = Some(now);
                    t.stage = TrialStage::Reviewing;
                    return true;
                }
                false
            });
        }
    }

    super::change(|f| {
        if let Some(t) = find_trial_mut(f, run_id, trial_id) {
            t.times.worker_done_at = Some(now);
            t.stage = TrialStage::Gating;
            return true;
        }
        false
    })
}

fn read_reply(engine: &Handle, cfg: &Config, key: &str, at: i64) -> Result<String> {
    let board = engine.board();
    let session = find_session(&board, key).ok_or_else(|| anyhow!("Session {key} isn't on the board."))?;
    let page = crate::transcript::page(&board, cfg, key, None, 60)?;
    autoreview::reply_after(&page.items, at, session.last_prompt_at).ok_or_else(|| anyhow!("No reply found for {key} after {at}."))
}

pub(crate) fn mark_done(engine: &Handle, key: &str) {
    let done = codealong_peer::api::params::Done { stop: true, archive: true, ..Default::default() };
    let _ = actions::local(engine, &From::Here, codealong_peer::api::SESSION_DONE, key, done);
}

fn exec_finish_review(engine: &Handle, cfg: &Config, run_id: &str, trial_id: &str) -> Result<()> {
    let file = super::load()?;
    let run = file.runs.iter().find(|r| r.id == run_id).ok_or_else(|| anyhow!("Run {run_id} not found."))?;
    let trial = run.trials.iter().find(|t| t.id == trial_id).ok_or_else(|| anyhow!("Trial {trial_id} not found."))?;
    let wt = trial.worktree.as_deref().ok_or_else(|| anyhow!("Trial {trial_id} has no worktree."))?;
    let rev_key = trial.reviewer_key.as_deref().ok_or_else(|| anyhow!("Trial {trial_id} has no reviewer session."))?;
    let sent = trial.turn_sent_at.unwrap_or(0);
    let tree_now = gitops::tree_hash(Path::new(wt))?;
    let modified = trial.tree_before_review.as_deref().is_some_and(|before| before != tree_now);
    let reply = read_reply(engine, cfg, rev_key, sent)?;
    let verdict = autoreview::verdict_of(&reply);
    let verdict_word = match verdict {
        Some(Verdict::Approve) => "approve",
        Some(Verdict::RequestChanges) => "request-changes",
        Some(Verdict::Comment) => "comment",
        None => "none",
    };
    let tdir = folder(run_id).join(trial_id);
    std::fs::create_dir_all(&tdir)?;
    let review_path = tdir.join("review-1.md");
    std::fs::write(&review_path, &reply)?;
    mark_done(engine, rev_key);

    let now = now_ms();
    if verdict == Some(Verdict::RequestChanges) {
        if let Some(worker_key) = trial.worker_key.as_deref() {
            let msg = prompts::fix_prompt(&review_path);
            let _ = crate::send::tell_waking(engine, worker_key, &msg)?;
            return super::change(|f| {
                if let Some(t) = find_trial_mut(f, run_id, trial_id) {
                    t.times.review_done_at = Some(now);
                    t.reviewer_modified_tree = modified;
                    t.review_verdict = Some(verdict_word.into());
                    t.turn_sent_at = Some(now);
                    t.stage = TrialStage::Fixing;
                    return true;
                }
                false
            });
        }
    }

    super::change(|f| {
        if let Some(t) = find_trial_mut(f, run_id, trial_id) {
            t.times.review_done_at = Some(now);
            t.reviewer_modified_tree = modified;
            t.review_verdict = Some(verdict_word.into());
            t.stage = TrialStage::Gating;
            return true;
        }
        false
    })
}

fn exec_finish_fix(run_id: &str, trial_id: &str) -> Result<()> {
    let now = now_ms();
    super::change(|f| {
        if let Some(t) = find_trial_mut(f, run_id, trial_id) {
            t.times.fix_done_at = Some(now);
            t.stage = TrialStage::Gating;
            return true;
        }
        false
    })
}

fn exec_run_gate(run_id: &str, trial_id: &str) -> Result<()> {
    let file = super::load()?;
    let run = file.runs.iter().find(|r| r.id == run_id).ok_or_else(|| anyhow!("Run {run_id} not found."))?;
    let trial = run.trials.iter().find(|t| t.id == trial_id).ok_or_else(|| anyhow!("Trial {trial_id} not found."))?;
    let wt = trial.worktree.as_deref().ok_or_else(|| anyhow!("Trial {trial_id} has no worktree."))?;

    // Capture the final snapshot BEFORE running the gate, which may create build artifacts.
    let snap = gitops::snapshot(Path::new(wt), &run.base_sha, Some((run_id, trial_id)))?;
    let tdir = folder(run_id).join(trial_id);
    std::fs::create_dir_all(&tdir)?;
    std::fs::write(tdir.join("diff.patch"), &snap.diff)?;
    let diff_bytes = snap.diff.len();

    let (gate_passed, gate_exit) = if let Some(gate) = &run.gate {
        let res = gitops::run_gate(Path::new(wt), &gate.cmd, gate.timeout_secs)?;
        std::fs::write(tdir.join("eval.txt"), &res.output)?;
        (Some(res.passed), Some(res.exit_code))
    } else {
        (None, None)
    };

    let now = now_ms();
    super::change(|f| {
        if let Some(t) = find_trial_mut(f, run_id, trial_id) {
            t.diff_ref = snap.git_ref;
            t.diff_bytes = diff_bytes;
            t.gate_passed = gate_passed;
            t.gate_exit = gate_exit;
            t.times.gate_done_at = Some(now);
            t.times.finished_at = Some(now);
            t.stage = TrialStage::Ready;
            return true;
        }
        false
    })
}

fn exec_start_judge(engine: &Handle, cfg: &Config, run_id: &str, pair_idx: usize, swapped: bool) -> Result<()> {
    let file = super::load()?;
    let run = file.runs.iter().find(|r| r.id == run_id).ok_or_else(|| anyhow!("Run {run_id} not found."))?;
    let judge = run.judge.as_ref().ok_or_else(|| anyhow!("Run {run_id} has no judge."))?;
    let cmp = run.comparisons.get(pair_idx).ok_or_else(|| anyhow!("Comparison {pair_idx} missing."))?;
    let ord = if swapped { &cmp.reversed } else { &cmp.forward };
    let ta = run.trials.iter().find(|t| t.id == ord.a_trial).ok_or_else(|| anyhow!("Trial {} missing.", ord.a_trial))?;
    let tb = run.trials.iter().find(|t| t.id == ord.b_trial).ok_or_else(|| anyhow!("Trial {} missing.", ord.b_trial))?;

    // Isolated scratch folder outside the repo and away from experiments.json so the judge is
    // strictly blind to model/harness names and git history.
    let dir = folder(run_id).join("judge").join(format!("p{pair_idx}-{}", if swapped { "rev" } else { "fwd" }));
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("spec.md"), &run.prompt)?;
    let raw_a = std::fs::read_to_string(folder(run_id).join(&ta.id).join("diff.patch")).unwrap_or_default();
    let raw_b = std::fs::read_to_string(folder(run_id).join(&tb.id).join("diff.patch")).unwrap_or_default();
    std::fs::write(dir.join("A.diff"), gitops::blind_diff(&raw_a, ta.worktree.as_deref()))?;
    std::fs::write(dir.join("B.diff"), gitops::blind_diff(&raw_b, tb.worktree.as_deref()))?;
    let has_gate = run.gate.is_some();
    if has_gate {
        let gate_txt = format!(
            "Candidate A gate: {}\nCandidate B gate: {}\n",
            if ta.gate_passed == Some(true) { "PASSED" } else { "FAILED" },
            if tb.gate_passed == Some(true) { "PASSED" } else { "FAILED" },
        );
        std::fs::write(dir.join("gate.txt"), gate_txt)?;
    }

    let acct = find_account(cfg, &judge.seat.account)?;
    let prompt = prompts::judge_prompt(has_gate);
    let now = now_ms();
    let key = if let Some(mode) = judge.seat.mode.as_deref() {
        let k = crate::acp::start(acct, &dir, "", judge.seat.model.as_deref(), judge.seat.effort.as_deref())?;
        crate::acp::switch_mode(acct, &k, mode)?;
        crate::acp::send(acct, &k, &prompt)?;
        k
    } else {
        crate::acp::start(acct, &dir, &prompt, judge.seat.model.as_deref(), judge.seat.effort.as_deref())?
    };
    engine.quiet(vec![key.clone()], true);

    super::change(|f| {
        if let Some(c) = f.runs.iter_mut().find(|r| r.id == run_id).and_then(|r| r.comparisons.get_mut(pair_idx)) {
            let slot = if swapped { &mut c.reversed } else { &mut c.forward };
            slot.session_key = Some(key);
            slot.started_at = Some(now);
            return true;
        }
        false
    })
}

fn exec_finish_judge(engine: &Handle, cfg: &Config, run_id: &str, pair_idx: usize, swapped: bool) -> Result<()> {
    let file = super::load()?;
    let run = file.runs.iter().find(|r| r.id == run_id).ok_or_else(|| anyhow!("Run {run_id} not found."))?;
    let cmp = run.comparisons.get(pair_idx).ok_or_else(|| anyhow!("Comparison {pair_idx} missing."))?;
    let ord = if swapped { &cmp.reversed } else { &cmp.forward };
    let key = ord.session_key.as_deref().ok_or_else(|| anyhow!("Judge session key missing."))?;
    let started = ord.started_at.unwrap_or(0);
    let reply = read_reply(engine, cfg, key, started)?;
    let winner = prompts::parse_winner(&reply).map(str::to_string);
    let dir = folder(run_id).join("judge").join(format!("p{pair_idx}-{}", if swapped { "rev" } else { "fwd" }));
    let _ = std::fs::write(dir.join("reply.md"), &reply);
    mark_done(engine, key);

    let rationale = reply.lines().rev().find(|l| !l.trim().is_empty() && !l.trim().to_ascii_lowercase().starts_with("winner:")).map(|s| s.trim().to_string());
    super::change(|f| {
        if let Some(c) = f.runs.iter_mut().find(|r| r.id == run_id).and_then(|r| r.comparisons.get_mut(pair_idx)) {
            let slot = if swapped { &mut c.reversed } else { &mut c.forward };
            slot.winner = winner;
            slot.rationale = rationale;
            slot.done = true;
            c.resolve();
            return true;
        }
        false
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use super::*;
    use crate::experiments::{Arm, Gate, JudgeCfg, PhaseTimes};
    use crate::model::{Lane, SessionLimit};

    const NOW: i64 = 1_795_000_000_000;

    fn board_with(lanes: Vec<Lane>) -> Board {
        Board { lanes, ..Default::default() }
    }

    fn lane(path: &str, sessions: Vec<Session>) -> Lane {
        Lane { path: path.into(), ..crate::fixtures::lane(path, sessions) }
    }

    fn sess(key: &str, status: Status, last_activity: i64) -> Session {
        Session { key: key.into(), status, last_activity, ..crate::fixtures::session(key) }
    }

    fn sample_run(n: usize) -> Run {
        let arms: Vec<Arm> = (1..=n).map(|i| Arm { label: format!("arm-{i}"), worker: Seat { account: "claude".into(), ..Default::default() }, ..Default::default() }).collect();
        super::super::make_run("bench".into(), "repo".into(), PathBuf::from("/r"), "abc1234".into(), "Fix bug".into(), arms, None, Some(JudgeCfg { seat: Seat { account: "codex".into(), ..Default::default() } }), 2)
    }

    #[test]
    fn plan_respects_max_parallel_and_waits_for_preparing_lane() {
        let mut run = sample_run(3);
        let (jobs, notes) = plan(&[run.clone()], &board_with(vec![]), &HashSet::new(), false, NOW);
        assert!(notes.is_empty());
        assert_eq!(jobs.len(), 2);
        assert!(matches!(jobs[0], Job::PrepareWorktree { ref trial_id, .. } if trial_id == "t1"));
        assert!(matches!(jobs[1], Job::PrepareWorktree { ref trial_id, .. } if trial_id == "t2"));

        // t1 is Preparing; until its lane appears on the board, StartWorker is not queued.
        run.trials[0].stage = TrialStage::Preparing;
        run.trials[0].worktree = Some("/r/.codealong/worktrees/wt1".into());
        run.trials[0].times.started_at = Some(NOW - 5_000);
        let (jobs, _) = plan(&[run.clone()], &board_with(vec![]), &HashSet::new(), false, NOW);
        assert!(!jobs.iter().any(|j| matches!(j, Job::StartWorker { .. })));

        // Once the lane appears, StartWorker is scheduled.
        let b = board_with(vec![lane("/r/.codealong/worktrees/wt1", vec![])]);
        let (jobs, _) = plan(&[run], &b, &HashSet::new(), false, NOW);
        assert!(jobs.iter().any(|j| matches!(j, Job::StartWorker { trial_id, .. } if trial_id == "t1")));
    }

    #[test]
    fn plan_does_not_end_on_cut_off_or_stale_turn_and_voids_on_limit() {
        let mut run = sample_run(2);
        run.trials[0].stage = TrialStage::Working;
        run.trials[0].worker_key = Some("claude:claude:w1".into());
        run.trials[0].turn_sent_at = Some(NOW);
        run.trials[1].stage = TrialStage::Working;
        run.trials[1].worker_key = Some("claude:claude:w2".into());
        run.trials[1].turn_sent_at = Some(NOW);

        // Stale idle from BEFORE turn_sent_at -> must NOT fire FinishWorker!
        let stale = sess("claude:claude:w1", Status::Idle, NOW - 100);
        // Cut off -> must NOT fire FinishWorker, only count interruption!
        let mut cut = sess("claude:claude:w2", Status::Stopped, NOW + 500);
        cut.cut_off = Some(NOW + 500);
        let b = board_with(vec![lane("/wt", vec![stale, cut])]);
        let (jobs, notes) = plan(&[run.clone()], &b, &HashSet::new(), false, NOW + 1000);
        assert!(jobs.is_empty(), "{jobs:?}");
        assert_eq!(notes, vec![Note::CountInterruption { run_id: run.id.clone(), trial_id: "t2".into() }]);

        // Usage limit -> voids the trial!
        let mut limited = sess("claude:claude:w1", Status::Stopped, NOW + 500);
        limited.limit = Some(SessionLimit { at: NOW + 500, ..Default::default() });
        let done_ok = sess("claude:claude:w2", Status::Unread, NOW + 500);
        let b = board_with(vec![lane("/wt", vec![limited, done_ok])]);
        let (jobs, notes) = plan(&[run.clone()], &b, &HashSet::new(), false, NOW + 1000);
        assert!(notes.iter().any(|n| matches!(n, Note::VoidTrial { trial_id, .. } if trial_id == "t1")));
        assert!(jobs.iter().any(|j| matches!(j, Job::FinishWorker { trial_id, .. } if trial_id == "t2")));
    }

    #[test]
    fn plan_runs_base_gate_first_and_only_one_gate_at_a_time() {
        let mut run = sample_run(2);
        run.gate = Some(Gate { cmd: "npm test".into(), timeout_secs: 60, check_base: true });
        let (jobs, _) = plan(&[run.clone()], &board_with(vec![]), &HashSet::new(), false, NOW);
        assert_eq!(jobs, vec![Job::CheckBaseGate { run_id: run.id.clone() }]);

        // Once base gate passed and both trials are Gating, only one RunGate schedules at a time.
        run.base_gate_ok = Some(true);
        run.trials[0].stage = TrialStage::Gating;
        run.trials[1].stage = TrialStage::Gating;
        let (jobs, _) = plan(&[run], &board_with(vec![]), &HashSet::new(), false, NOW);
        assert_eq!(jobs.len(), 1);
        assert!(matches!(jobs[0], Job::RunGate { ref trial_id, .. } if trial_id == "t1"));
    }

    #[test]
    fn plan_transitions_to_judging_and_schedules_both_orders_sequentially() {
        let mut run = sample_run(2);
        for t in &mut run.trials {
            t.stage = TrialStage::Ready;
            t.diff_bytes = 200;
            t.times = PhaseTimes { queued_at: NOW - 1000, finished_at: Some(NOW), ..Default::default() };
        }
        let (_, notes) = plan(&[run.clone()], &board_with(vec![]), &HashSet::new(), false, NOW);
        assert!(matches!(&notes[0], Note::BeginJudging { comparisons, .. } if comparisons.len() == 1));

        let mut file = StoreFile { runs: vec![run] };
        apply_notes(&mut file, &notes);
        assert_eq!(file.runs[0].stage, RunStage::Judging);

        // Schedules forward order first, not both orders simultaneously.
        let (jobs, _) = plan(&file.runs, &board_with(vec![]), &HashSet::new(), false, NOW);
        assert_eq!(jobs, vec![Job::StartJudge { run_id: file.runs[0].id.clone(), pair_idx: 0, swapped: false }]);

        // Forward completes -> schedules reversed.
        file.runs[0].comparisons[0].forward.done = true;
        file.runs[0].comparisons[0].forward.winner = Some("A".into());
        let (jobs, _) = plan(&file.runs, &board_with(vec![]), &HashSet::new(), false, NOW);
        assert_eq!(jobs, vec![Job::StartJudge { run_id: file.runs[0].id.clone(), pair_idx: 0, swapped: true }]);

        // Both done -> CompleteRun with recommendation.
        file.runs[0].comparisons[0].reversed.done = true;
        file.runs[0].comparisons[0].reversed.winner = Some("B".into());
        file.runs[0].comparisons[0].resolve();
        let (jobs, notes) = plan(&file.runs, &board_with(vec![]), &HashSet::new(), false, NOW + 10);
        assert!(jobs.is_empty());
        assert!(matches!(&notes[0], Note::CompleteRun { recommended: Some(id), .. } if id == "t1"));
    }
}
