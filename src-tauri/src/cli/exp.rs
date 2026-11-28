//! `codealong exp`: running pinned agent combinations side-by-side on the same task and base
//! commit, optionally with a 1-round local reviewer, a mechanical gate command, and a blind
//! pairwise judge.
//!
//! Gated behind `[experimental] experiments = true` (Settings › Experimental). `exp run` validates
//! the arms and writes a queued run into `experiments.json` for Codealong's server runner
//! (`experiments/runner.rs`) to execute; `adopt`, `discard` and `cancel` ask the server's
//! `experiment_op` so active sessions and worktrees are cleaned up cleanly.

use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use clap::{ArgAction, Args, Subcommand};
use serde_json::{json, Value};

use crate::experiments::{self, gitops, report, Gate, JudgeCfg, JudgeMode, Op};
use crate::server::{self, Request};
use crate::util::{expand, file_name};

use super::*;

#[derive(Subcommand, Debug)]
pub(super) enum ExpCmd {
    /// Start a bake-off run across pinned agent combinations on the same task and base commit.
    Run(ExpRunArgs),
    /// List named experiment presets configured under `[experiments.<name>]` in config.toml.
    Presets,
    /// List experiment runs.
    List {
        /// Only runs in this named experiment.
        #[arg(long)]
        experiment: Option<String>,
    },
    /// Show one experiment run in full (trials, gate, judges' rankings or pairwise comparisons, costs).
    Show {
        /// The run id (or a unique prefix).
        id: String,
    },
    /// Show the leaderboard aggregated across completed runs.
    Results {
        /// Only runs in this named experiment.
        experiment: Option<String>,
    },
    /// Mark a trial as the adopted winner (recording whether you agreed with the judge).
    Adopt {
        /// Run id (or `<run-id>/<trial-id>`).
        run: String,
        /// Trial id (e.g. `t1`), unless given after `/` in RUN.
        trial: Option<String>,
    },
    /// Save diffs to git refs and patch files, mark trial sessions done, and force-remove worktrees.
    Discard {
        /// Run id (or `<run-id>/<trial-id>`). Without a trial id, discards every non-adopted trial.
        run: String,
        /// Only discard this trial id.
        trial: Option<String>,
    },
    /// Stop an active run's hosted sessions and mark the run cancelled.
    Cancel {
        /// Run id (or unique prefix).
        id: String,
    },
}

#[derive(Args, Debug)]
pub(super) struct ExpRunArgs {
    /// Named preset from `[experiments.<name>]` in config.toml (supplies arms, judge, rubric, and optional eval).
    #[arg(long)]
    pub preset: Option<String>,
    /// Arm specification `<worker>[+<reviewer>]`, where each seat is `<account>[:<model>][:<effort>][@<mode>]`. Pass at least twice unless `--preset` supplies arms.
    #[arg(long = "arm", action = ArgAction::Append)]
    pub arms: Vec<String>,
    /// Blind judge seat `<account>[:<model>][:<effort>][@<mode>]`. Repeat for a multi-provider judge panel (overrides preset judge/judges).
    #[arg(long = "judge", action = ArgAction::Append, conflicts_with = "no_judge")]
    pub judges: Vec<String>,
    /// Skip the preset's judge(s) for this run.
    #[arg(long, conflicts_with = "judges")]
    pub no_judge: bool,
    /// How judges compare surviving arms: `auto` (all-at-once ranking for 3+, pairwise for 2), `rank`, or `pairwise`.
    #[arg(long, value_enum)]
    pub judge_mode: Option<JudgeMode>,
    /// Extra evaluation criteria appended to the blind judge's prompt (overrides preset rubric).
    #[arg(long)]
    pub rubric: Option<String>,
    /// Optional mechanical gate command run through your login shell in each trial worktree after its last agent turn.
    #[arg(long = "eval", conflicts_with = "no_eval")]
    pub eval_cmd: Option<String>,
    /// Skip the preset's mechanical gate command for this run.
    #[arg(long, conflicts_with = "eval_cmd")]
    pub no_eval: bool,
    /// Timeout in seconds for each gate execution (default 600, or preset's `eval_timeout`).
    #[arg(long)]
    pub eval_timeout: Option<u64>,
    /// Skip running --eval once on a clean base worktree before launching arms.
    #[arg(long)]
    pub skip_base_check: bool,
    /// The task prompt. Or --prompt-file or --todo.
    #[arg(long, conflicts_with_all = ["prompt_file", "todo"])]
    pub prompt: Option<String>,
    /// Read the task prompt from a file, or `-` for stdin.
    #[arg(long, conflicts_with_all = ["prompt", "todo"])]
    pub prompt_file: Option<String>,
    /// Read the task prompt from a todo's title and description (without linking arms to the todo).
    #[arg(long, conflicts_with_all = ["prompt", "prompt_file"])]
    pub todo: Option<String>,
    /// Named experiment cohort for grouping leaderboard results (defaults to the preset name, or `default`).
    #[arg(long)]
    pub experiment: Option<String>,
    /// Repository folder to base trial worktrees on (default `.`).
    #[arg(long, default_value = ".")]
    pub worktree: String,
    /// Git ref to pin every arm's starting commit to (default `HEAD`).
    #[arg(long, default_value = "HEAD")]
    pub base: String,
    /// Maximum number of arms running concurrently (default 2, or preset's `max_parallel`).
    #[arg(long)]
    pub max_parallel: Option<usize>,
    /// Validate arms, boundaries, and base commit and print the plan without starting anything.
    #[arg(long)]
    pub dry_run: bool,
}

pub(super) fn exp(cmd: ExpCmd, c: &mut Client, json_out: bool) -> Result<()> {
    let cfg = c.cfg()?.clone();
    if !cfg.experimental.experiments {
        bail!("Experiments are off. Turn them on in Settings › Experimental (`[experimental] experiments = true` in config.toml).");
    }
    match cmd {
        ExpCmd::Run(a) => exp_run(a, c, &cfg, json_out),
        ExpCmd::Presets => exp_presets(&cfg, json_out),
        ExpCmd::List { experiment } => exp_list(&cfg, experiment.as_deref(), json_out),
        ExpCmd::Show { id } => exp_show(&cfg, &id, json_out),
        ExpCmd::Results { experiment } => exp_results(&cfg, experiment.as_deref(), json_out),
        ExpCmd::Adopt { run, trial } => {
            let (run_id, trial_id) = split_run_trial(&run, trial.as_deref())?;
            let run_id = resolve_run_id(&run_id)?;
            let out = run_exp_op(Op::Adopt { run_id, trial_id })?;
            if json_out {
                return print_json(&out);
            }
            println!("Adopted {} in {} ({}).", out["trialId"].as_str().unwrap_or(""), out["runId"].as_str().unwrap_or(""), out["worktree"].as_str().unwrap_or(""));
            Ok(())
        }
        ExpCmd::Discard { run, trial } => {
            let (run_id, trial_id) = match (run.split_once('/'), trial) {
                (Some((r, t)), None) => (resolve_run_id(r)?, Some(t.to_string())),
                (_, t) => (resolve_run_id(&run)?, t),
            };
            let out = run_exp_op(Op::Discard { run_id, trial_id })?;
            if json_out {
                return print_json(&out);
            }
            println!("Discarded trials in {}: {}", out["runId"].as_str().unwrap_or(""), out["discarded"]);
            Ok(())
        }
        ExpCmd::Cancel { id } => {
            let run_id = resolve_run_id(&id)?;
            let out = run_exp_op(Op::Cancel { run_id })?;
            if json_out {
                return print_json(&out);
            }
            println!("Cancelled {}.", out["runId"].as_str().unwrap_or(""));
            Ok(())
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ResolvedRun {
    experiment: String,
    arms: Vec<String>,
    judges: Vec<String>,
    judge_mode: JudgeMode,
    rubric: Option<String>,
    eval_cmd: Option<String>,
    eval_timeout: u64,
    skip_base_check: bool,
    max_parallel: usize,
}

fn resolve_run_args(a: &ExpRunArgs, cfg: &crate::config::Config) -> Result<ResolvedRun> {
    let preset = match a.preset.as_deref() {
        Some(name) => Some(cfg.experiments.get(name).ok_or_else(|| {
            if cfg.experiments.is_empty() {
                anyhow!("No experiment preset {name:?}. Add `[experiments.{name}]` to config.toml.")
            } else {
                let names: Vec<&str> = cfg.experiments.keys().map(String::as_str).collect();
                anyhow!("No experiment preset {name:?}. Configured presets: {}.", names.join(", "))
            }
        })?),
        None => None,
    };

    let arms = if !a.arms.is_empty() {
        a.arms.clone()
    } else if let Some(p) = preset {
        p.arms.clone()
    } else {
        bail!("Give at least two --arm entries, or choose a --preset from `[experiments.<name>]` in config.toml.");
    };

    let judges: Vec<String> = if a.no_judge {
        Vec::new()
    } else if !a.judges.is_empty() {
        a.judges.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    } else if let Some(p) = preset {
        p.resolved_judges()
    } else {
        Vec::new()
    };
    let judge_mode = a.judge_mode.or_else(|| preset.and_then(|p| p.judge_mode)).unwrap_or(JudgeMode::Auto);
    let cli_rubric = a.rubric.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    if cli_rubric.is_some() && judges.is_empty() {
        bail!("--rubric requires a --judge (or a preset with `judge` / `judges`).");
    }
    let rubric = if !judges.is_empty() {
        cli_rubric.or_else(|| preset.and_then(|p| p.rubric.as_deref()).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string))
    } else {
        None
    };

    let eval_cmd = if a.no_eval {
        None
    } else {
        a.eval_cmd.clone().or_else(|| preset.and_then(|p| p.eval.clone())).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    };
    let eval_timeout = a.eval_timeout.or_else(|| preset.and_then(|p| p.eval_timeout)).unwrap_or(600);
    let skip_base_check = a.skip_base_check || preset.is_some_and(|p| p.skip_base_check);
    let max_parallel = a.max_parallel.or_else(|| preset.and_then(|p| p.max_parallel)).unwrap_or(2);
    if max_parallel == 0 {
        bail!("--max-parallel must be at least 1.");
    }
    let experiment = a
        .experiment
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| a.preset.clone())
        .unwrap_or_else(|| "default".into());

    Ok(ResolvedRun { experiment, arms, judges, judge_mode, rubric, eval_cmd, eval_timeout, skip_base_check, max_parallel })
}

fn exp_presets(cfg: &crate::config::Config, json_out: bool) -> Result<()> {
    if json_out {
        return print_json(&cfg.experiments);
    }
    if cfg.experiments.is_empty() {
        println!("No experiment presets configured. Add `[experiments.<name>]` in config.toml.");
        return Ok(());
    }
    let rows: Vec<Vec<String>> = cfg
        .experiments
        .iter()
        .map(|(name, p)| {
            let judges = p.resolved_judges();
            let judge_col = if judges.is_empty() { "—".into() } else { judges.join(", ") };
            vec![
                name.clone(),
                p.arms.join(", "),
                judge_col,
                p.rubric.as_deref().map(|s| {
                    let line = s.lines().next().unwrap_or("").trim();
                    if line.chars().count() > 48 { format!("{}…", line.chars().take(47).collect::<String>()) } else { line.to_string() }
                }).unwrap_or_else(|| "—".into()),
                p.eval.clone().unwrap_or_else(|| "—".into()),
            ]
        })
        .collect();
    print!("{}", table_text(&["Preset", "Arms", "Judge(s)", "Rubric", "Eval"], rows));
    Ok(())
}

fn exp_run(a: ExpRunArgs, c: &mut Client, cfg: &crate::config::Config, json_out: bool) -> Result<()> {
    let resolved = resolve_run_args(&a, cfg)?;
    let prompt = read_task_prompt(&a, c)?;
    let folder = resolve_repo_folder(&a.worktree)?;
    let (_, main) = crate::repos::locate(&folder).ok_or_else(|| anyhow!("{} isn't in a git repository.", folder.display()))?;
    let base_sha = gitops::resolve_sha(&main, &a.base)?;
    let repo = file_name(&main);

    let arms: Vec<experiments::Arm> = resolved.arms.iter().map(|raw| experiments::parse_arm(cfg, raw)).collect::<Result<_>>()?;
    let judge_seats: Vec<experiments::Seat> = resolved.judges.iter().map(|raw| experiments::parse_seat(cfg, raw)).collect::<Result<_>>()?;
    let judge = JudgeCfg::from_seats(judge_seats, resolved.judge_mode, resolved.rubric.clone());
    let warnings = experiments::validate_run(cfg, &main, &arms, judge.as_ref())?;
    for w in &warnings {
        eprintln!("warning: {w}");
    }
    let gate = resolved.eval_cmd.map(|cmd| Gate { cmd, timeout_secs: resolved.eval_timeout, check_base: !resolved.skip_base_check });
    let run = experiments::make_run(resolved.experiment, repo, main, base_sha, prompt, arms, gate, judge, resolved.max_parallel);

    if a.dry_run {
        if json_out {
            return print_json(&json!({ "dryRun": true, "warnings": warnings, "run": run }));
        }
        println!("Dry run {} ({}) at {}:", run.id, run.experiment, &run.base_sha[..run.base_sha.len().min(8)]);
        for t in &run.trials {
            println!("  {}  {}", t.id, t.arm.label);
        }
        if let Some(j) = &run.judge {
            let names: Vec<String> = j.resolved_seats().into_iter().map(|s| s.short()).collect();
            let mode_word = if j.use_rank(run.trials.len()) { "rank" } else { "pairwise" };
            println!("  judges {} ({mode_word})", names.join(", "));
            if let Some(r) = &j.rubric {
                println!("  rubric {r}");
            }
        }
        if let Some(g) = &run.gate {
            println!("  eval   {}", g.cmd);
        }
        return Ok(());
    }

    let id = run.id.clone();
    let run_out = run.clone();
    experiments::change(|f| {
        f.runs.push(run);
        true
    })?;
    // Ensure the server is running so the experiment runner picks up the queued run immediately.
    let _ = c.board();

    if json_out {
        return print_json(&json!({ "run": run_out, "warnings": warnings }));
    }
    println!("Queued experiment run {} ({}, {} arms at {}).", id, run_out.experiment, run_out.trials.len(), &run_out.base_sha[..run_out.base_sha.len().min(8)]);
    println!("Watch progress with `codealong exp show {id}`.");
    Ok(())
}

fn exp_list(cfg: &crate::config::Config, experiment: Option<&str>, json_out: bool) -> Result<()> {
    let rep = report::build(cfg, experiment)?;
    if json_out {
        return print_json(&rep.runs);
    }
    if rep.runs.is_empty() {
        println!("No experiment runs yet.");
        return Ok(());
    }
    let rows: Vec<Vec<String>> = rep
        .runs
        .iter()
        .map(|r| {
            let cost: f64 = r.trials.iter().map(|t| t.cost.total_api).sum::<f64>() + r.judge_cost_api;
            vec![
                r.id.clone(),
                r.experiment.clone(),
                format!("{:?}", r.stage).to_lowercase(),
                format!("{} arms", r.trials.len()),
                r.recommended.clone().unwrap_or_else(|| "—".into()),
                format!("${cost:.2}"),
            ]
        })
        .collect();
    print!("{}", table_text(&["Run", "Experiment", "Stage", "Arms", "Recommended", "Cost"], rows));
    Ok(())
}

fn exp_show(cfg: &crate::config::Config, prefix: &str, json_out: bool) -> Result<()> {
    let id = resolve_run_id(prefix)?;
    let rep = report::build(cfg, None)?;
    let run = rep.runs.iter().find(|r| r.id == id).ok_or_else(|| anyhow!("No experiment run {prefix}."))?;
    if json_out {
        return print_json(run);
    }
    println!("{}  {}  ({:?})  {}@{}", run.id, run.experiment, run.stage, run.repo, &run.base_sha[..run.base_sha.len().min(8)]);
    if let Some(note) = &run.recommendation_note {
        println!("Result: {note}");
    }
    if let Some(err) = &run.error {
        println!("Error: {err}");
    }
    let rows: Vec<Vec<String>> = run
        .trials
        .iter()
        .map(|t| {
            let gate = match t.gate_passed {
                Some(true) => "pass".into(),
                Some(false) => "fail".into(),
                None => "—".into(),
            };
            let mark = if t.adopted { "adopted" } else if t.recommended { "recommended" } else if t.discarded { "discarded" } else { "" };
            vec![
                t.id.clone(),
                t.label.clone(),
                format!("{:?}", t.stage).to_lowercase(),
                t.review_verdict.clone().unwrap_or_else(|| "—".into()),
                gate,
                format!("${:.2}", t.cost.total_api),
                t.duration_ms.map(fmt_dur).unwrap_or_else(|| "—".into()),
                mark.into(),
            ]
        })
        .collect();
    print!("{}", table_text(&["Trial", "Arm", "Stage", "Review", "Gate", "Cost", "Time", ""], rows));
    if !run.rankings.is_empty() {
        println!("\nJudge rankings:");
        for rj in &run.rankings {
            let order = if !rj.ranking.is_empty() {
                rj.ranking.iter().map(|tier| tier.join(" = ")).collect::<Vec<_>>().join(" > ")
            } else if let Some(err) = &rj.error {
                format!("error: {err}")
            } else if rj.done {
                "no ranking".into()
            } else {
                "judging…".into()
            };
            println!("  {}  →  {}", rj.judge, order);
            if let Some(why) = &rj.rationale {
                println!("    {why}");
            }
        }
    }
    if !run.comparisons.is_empty() {
        println!("\nJudge comparisons:");
        for c in &run.comparisons {
            let winner = c.consistent_winner.as_deref().unwrap_or("no consensus");
            let who = c.judge.as_deref().map(|j| format!("[{j}] ")).unwrap_or_default();
            println!(
                "  {who}{} vs {}  →  {} (fwd: {}, rev: {})",
                c.trial_a,
                c.trial_b,
                winner,
                c.forward_winner.as_deref().unwrap_or("tie"),
                c.reversed_winner.as_deref().unwrap_or("tie"),
            );
            if let Some(why) = &c.rationale {
                println!("    {why}");
            }
        }
    }
    Ok(())
}

fn exp_results(cfg: &crate::config::Config, experiment: Option<&str>, json_out: bool) -> Result<()> {
    let rep = report::build(cfg, experiment)?;
    if json_out {
        return print_json(&rep);
    }
    if rep.leaderboard.is_empty() {
        println!("No completed experiment trials yet.");
        return Ok(());
    }
    let rows: Vec<Vec<String>> = rep
        .leaderboard
        .iter()
        .map(|r| {
            let gate = if r.gate_total > 0 { format!("{}/{}", r.gate_passed, r.gate_total) } else { "—".into() };
            vec![
                r.experiment.clone(),
                r.label.clone(),
                r.runs.to_string(),
                format!("{}-{}-{}", r.wins, r.losses, r.ties),
                gate,
                format!("${:.2}", r.avg_cost_api),
                r.avg_duration_ms.map(fmt_dur).unwrap_or_else(|| "—".into()),
            ]
        })
        .collect();
    print!("{}", table_text(&["Experiment", "Arm", "Runs", "W-L-T", "Gate", "Avg Cost", "Avg Time"], rows));
    Ok(())
}

fn run_exp_op(op: Op) -> Result<Value> {
    let req = Request::ExperimentOp { op: op.clone() };
    match server::ask(&req) {
        Some(Ok(v)) => Ok(v),
        Some(Err(e)) => bail!("{e}"),
        None => report::apply_op(None, op),
    }
}

fn read_task_prompt(a: &ExpRunArgs, c: &mut Client) -> Result<String> {
    if let Some(p) = &a.prompt {
        if p.trim().is_empty() {
            bail!("--prompt can't be empty.");
        }
        return Ok(p.clone());
    }
    if let Some(path) = &a.prompt_file {
        let text = if path == "-" {
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s).context("reading prompt from stdin")?;
            s
        } else {
            std::fs::read_to_string(expand(path)).with_context(|| format!("reading {path}"))?
        };
        if text.trim().is_empty() {
            bail!("Prompt file {path} is empty.");
        }
        return Ok(text);
    }
    if let Some(tid) = &a.todo {
        let store = c.todos()?;
        let matches: Vec<_> = store.todos.iter().filter(|t| t.id == *tid || t.id.starts_with(tid)).collect();
        let todo = match matches.as_slice() {
            [one] => *one,
            [] => bail!("No todo matching {tid:?}."),
            _ => bail!("More than one todo starts with {tid:?}."),
        };
        let body = todo.description.trim();
        return Ok(if body.is_empty() { todo.title.clone() } else { format!("{}\n\n{}", todo.title, body) });
    }
    bail!("Give a task prompt with --prompt, --prompt-file, or --todo.");
}

fn resolve_repo_folder(raw: &str) -> Result<PathBuf> {
    let p = if raw == "." { std::env::current_dir()? } else { expand(raw) };
    if !p.is_dir() {
        bail!("{} isn't a folder.", p.display());
    }
    Ok(dunce::canonicalize(&p).unwrap_or(p))
}

fn resolve_run_id(prefix: &str) -> Result<String> {
    let file = experiments::load()?;
    let matches: Vec<&str> = file.runs.iter().map(|r| r.id.as_str()).filter(|id| *id == prefix || id.starts_with(prefix)).collect();
    match matches.as_slice() {
        [one] => Ok((*one).to_string()),
        [] => bail!("No experiment run matching {prefix:?}."),
        _ => bail!("More than one experiment run starts with {prefix:?}."),
    }
}

fn split_run_trial(run: &str, trial: Option<&str>) -> Result<(String, String)> {
    match (run.split_once('/'), trial) {
        (Some((r, t)), None) => Ok((r.to_string(), t.to_string())),
        (None, Some(t)) => Ok((run.to_string(), t.to_string())),
        _ => bail!("Give both a run id and a trial id (`<run> <trial>` or `<run>/<trial>`)."),
    }
}

fn fmt_dur(ms: i64) -> String {
    let s = (ms / 1000).max(0);
    let m = s / 60;
    let h = m / 60;
    if h > 0 {
        format!("{h}h {}m", m % 60)
    } else if m > 0 {
        format!("{m}m {}s", s % 60)
    } else {
        format!("{s}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exp_cli_parses_arms_judge_eval_and_subcommands() {
        let cli = Cli::try_parse_from([
            "codealong", "exp", "run",
            "--arm", "claude:opus:high+codex:sol-6.2:high",
            "--arm", "codex:sol-6.2:high",
            "--judge", "antigravity:gemini-4-neon-high",
            "--eval", "npm test",
            "--prompt", "Fix the bug",
            "--experiment", "bench-1",
            "--dry-run",
        ])
        .unwrap();
        let Cmd::Exp { cmd: ExpCmd::Run(a) } = cli.cmd else { panic!("expected exp run") };
        assert_eq!(a.arms, vec!["claude:opus:high+codex:sol-6.2:high", "codex:sol-6.2:high"]);
        assert_eq!(a.judges, vec!["antigravity:gemini-4-neon-high"]);
        assert_eq!(a.eval_cmd.as_deref(), Some("npm test"));
        assert_eq!(a.experiment.as_deref(), Some("bench-1"));
        assert!(a.dry_run);

        let adopt = Cli::try_parse_from(["codealong", "exp", "adopt", "exp-1/t2"]).unwrap();
        assert!(matches!(adopt.cmd, Cmd::Exp { cmd: ExpCmd::Adopt { ref run, trial: None } } if run == "exp-1/t2"));
        assert_eq!(split_run_trial("exp-1/t2", None).unwrap(), ("exp-1".into(), "t2".into()));
        assert_eq!(split_run_trial("exp-1", Some("t2")).unwrap(), ("exp-1".into(), "t2".into()));
        assert!(split_run_trial("exp-1", None).is_err());
        let presets = Cli::try_parse_from(["codealong", "exp", "presets"]).unwrap();
        assert!(matches!(presets.cmd, Cmd::Exp { cmd: ExpCmd::Presets }));
    }

    #[test]
    fn preset_resolution_works_without_eval_and_allows_cli_overrides() {
        let mut cfg = crate::config::Config::default();
        cfg.experiments.insert(
            "quality".into(),
            experiments::PresetCfg {
                arms: vec!["claude:opus:xhigh".into(), "codex:gpt-6.2-astra:xhigh".into(), "antigravity:gemini-4-neon-xhigh".into()],
                judges: vec!["claude:opus:high".into(), "codex:gpt-6.2-astra:high".into(), "antigravity:gemini-4-neon-high".into()],
                rubric: Some("Prefer idiomatic reuse of existing helpers.".into()),
                eval: None,
                max_parallel: Some(3),
                ..Default::default()
            },
        );
        let cli = Cli::try_parse_from(["codealong", "exp", "run", "--preset", "quality", "--prompt", "Refactor auth"]).unwrap();
        let Cmd::Exp { cmd: ExpCmd::Run(a) } = cli.cmd else { panic!("expected exp run") };
        let res = resolve_run_args(&a, &cfg).unwrap();
        assert_eq!(res.experiment, "quality");
        assert_eq!(res.arms.len(), 3);
        assert_eq!(res.judges, vec!["claude:opus:high", "codex:gpt-6.2-astra:high", "antigravity:gemini-4-neon-high"]);
        assert_eq!(res.judge_mode, JudgeMode::Auto);
        assert_eq!(res.rubric.as_deref(), Some("Prefer idiomatic reuse of existing helpers."));
        assert_eq!(res.eval_cmd, None);
        assert_eq!(res.max_parallel, 3);

        // CLI overrides preset fields (--no-judge drops judge & preset rubric; --experiment overrides cohort).
        let cli2 = Cli::try_parse_from([
            "codealong", "exp", "run",
            "--preset", "quality",
            "--no-judge",
            "--experiment", "custom-cohort",
            "--prompt", "Quick test",
        ])
        .unwrap();
        let Cmd::Exp { cmd: ExpCmd::Run(a2) } = cli2.cmd else { panic!("expected exp run") };
        let res2 = resolve_run_args(&a2, &cfg).unwrap();
        assert_eq!(res2.experiment, "custom-cohort");
        assert!(res2.judges.is_empty());
        assert_eq!(res2.rubric, None);

        // Passing --rubric without any judge is rejected cleanly.
        let cli3 = Cli::try_parse_from([
            "codealong", "exp", "run",
            "--arm", "claude", "--arm", "codex",
            "--rubric", "Prefer short diffs",
            "--prompt", "Task",
        ])
        .unwrap();
        let Cmd::Exp { cmd: ExpCmd::Run(a3) } = cli3.cmd else { panic!("expected exp run") };
        assert!(resolve_run_args(&a3, &cfg).unwrap_err().to_string().contains("--rubric"));
    }
}
