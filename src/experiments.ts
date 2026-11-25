// Pure formatting and sorting helpers for the Analytics window's Experiments tab
// (`ui/UsageExperiments.tsx`).

import type { ExperimentLeaderboardRow, ExperimentRunView, ExperimentTrialStage, ExperimentTrialView } from './types';

/** Formats a trial or run duration in milliseconds (`1h 23m`, `4m 12s`, `18s`). */
export function formatDuration(ms: number | null | undefined): string {
  if (ms == null || ms < 0) return '—';
  const totalSec = Math.floor(ms / 1000);
  const h = Math.floor(totalSec / 3600);
  const m = Math.floor((totalSec % 3600) / 60);
  const s = totalSec % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

/** Human-readable label for a trial stage. */
export function stageLabel(stage: ExperimentTrialStage): string {
  switch (stage) {
    case 'queued':
      return 'Queued';
    case 'preparing':
      return 'Preparing';
    case 'working':
      return 'Working';
    case 'reviewing':
      return 'Reviewing';
    case 'fixing':
      return 'Fixing';
    case 'gating':
      return 'Gating';
    case 'ready':
      return 'Ready';
    case 'failed':
      return 'Failed';
    case 'voided':
      return 'Voided';
    default:
      return stage;
  }
}

/** Short record string for pairwise wins, losses and ties (`3W · 1L · 1T`). */
export function recordLabel(r: Pick<ExperimentLeaderboardRow, 'wins' | 'losses' | 'ties'>): string {
  const decided = r.wins + r.losses + r.ties;
  if (decided === 0) return '—';
  return r.ties > 0 ? `${r.wins}W · ${r.losses}L · ${r.ties}T` : `${r.wins}W · ${r.losses}L`;
}

/** Total API cost of a run (all trials plus judge calls). */
export function runTotalCost(run: ExperimentRunView): number {
  return run.trials.reduce((sum, t) => sum + t.cost.totalApi, 0) + run.judgeCostApi;
}

/** Concise flags noting warnings or anomalies on a trial. */
export function trialFlags(t: ExperimentTrialView): string[] {
  const out: string[] = [];
  if (t.adopted) out.push('Adopted');
  else if (t.recommended) out.push('Recommended');
  if (t.discarded) out.push('Discarded');
  if (t.prOpened) out.push('PR opened');
  if (t.reviewerModifiedTree) out.push('Reviewer edited tree');
  if (t.drift) out.push(t.drift);
  if (t.interruptions > 0) out.push(`${t.interruptions} cut-off`);
  return out;
}

/** Filters leaderboard rows to one named experiment (or all when `experiment` is empty). */
export function filterLeaderboard(rows: ExperimentLeaderboardRow[], experiment: string): ExperimentLeaderboardRow[] {
  if (!experiment) return rows;
  return rows.filter((r) => r.experiment === experiment);
}

/** Filters runs to one named experiment (or all when `experiment` is empty). */
export function filterRuns(runs: ExperimentRunView[], experiment: string): ExperimentRunView[] {
  if (!experiment) return runs;
  return runs.filter((r) => r.experiment === experiment);
}
