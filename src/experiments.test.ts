import { describe, expect, it } from 'vitest';
import { filterLeaderboard, filterRuns, formatDuration, recordLabel, runTotalCost, stageLabel, trialFlags } from './experiments';
import type { ExperimentLeaderboardRow, ExperimentRunView, ExperimentTrialView } from './types';

function trial(over: Partial<ExperimentTrialView> = {}): ExperimentTrialView {
  return {
    id: 't1',
    label: 'claude/opus + codex/sol',
    worker: 'claude/opus',
    reviewer: 'codex/sol',
    observedModel: 'claude-opus-5-5',
    stage: 'ready',
    worktree: '/r/.codealong/worktrees/w1',
    branch: 'worktree-w1',
    workerKey: 'claude:claude:w1',
    reviewerKey: 'codex:codex:r1',
    diffBytes: 1200,
    diffRef: 'refs/codealong/experiments/exp-1/t1',
    reviewVerdict: 'approve',
    reviewerModifiedTree: false,
    gatePassed: true,
    prOpened: false,
    drift: null,
    error: null,
    interruptions: 0,
    durationMs: 95_000,
    cost: { totalApi: 1.5, workerApi: 1.1, reviewerApi: 0.4, inputTokens: 1000, outputTokens: 200, cacheReadTokens: 0, unpricedCalls: 0 },
    adopted: false,
    discarded: false,
    recommended: false,
    ...over,
  };
}

describe('experiments helpers', () => {
  it('formats durations cleanly across seconds, minutes and hours', () => {
    expect(formatDuration(null)).toBe('—');
    expect(formatDuration(18_000)).toBe('18s');
    expect(formatDuration(252_000)).toBe('4m 12s');
    expect(formatDuration(4_980_000)).toBe('1h 23m');
  });

  it('formats pairwise win-loss-tie records and stage labels', () => {
    expect(recordLabel({ wins: 0, losses: 0, ties: 0 })).toBe('—');
    expect(recordLabel({ wins: 3, losses: 1, ties: 0 })).toBe('3W · 1L');
    expect(recordLabel({ wins: 2, losses: 1, ties: 1 })).toBe('2W · 1L · 1T');
    expect(stageLabel('reviewing')).toBe('Reviewing');
  });

  it('computes run total cost and trial warning flags', () => {
    const run: ExperimentRunView = {
      id: 'exp-1',
      experiment: 'bench',
      repo: 'codealong',
      baseSha: 'abc1234',
      prompt: 'Fix parser',
      stage: 'complete',
      createdAt: 1000,
      finishedAt: 5000,
      durationMs: 4000,
      baseGateOk: true,
      recommended: 't1',
      recommendationNote: 'Recommended',
      adoptedTrial: null,
      judgeCostApi: 0.35,
      error: null,
      trials: [trial(), trial({ id: 't2', cost: { totalApi: 0.9, workerApi: 0.9, reviewerApi: 0, inputTokens: 0, outputTokens: 0, cacheReadTokens: 0, unpricedCalls: 0 } })],
      comparisons: [],
      rankings: [],
    };
    expect(runTotalCost(run)).toBeCloseTo(2.75);
    expect(trialFlags(trial({ recommended: true, prOpened: true, reviewerModifiedTree: true, interruptions: 1 }))).toEqual([
      'Recommended',
      'PR opened',
      'Reviewer edited tree',
      '1 cut-off',
    ]);
  });

  it('filters leaderboard rows and runs by experiment name', () => {
    const row = (experiment: string): ExperimentLeaderboardRow => ({
      key: experiment,
      experiment,
      workerHarness: 'claude',
      workerModel: 'opus',
      workerEffort: 'high',
      reviewer: null,
      label: 'claude/opus/high',
      runs: 1,
      wins: 1,
      ties: 0,
      losses: 0,
      gatePassed: 1,
      gateTotal: 1,
      avgCostApi: 2.0,
      avgWorkerCostApi: 2.0,
      avgReviewerCostApi: 0,
      avgDurationMs: 60_000,
      adoptedCount: 0,
    });
    expect(filterLeaderboard([row('a'), row('b')], 'a')).toHaveLength(1);
    expect(filterLeaderboard([row('a'), row('b')], '')).toHaveLength(2);
    const run = (experiment: string): ExperimentRunView => ({
      id: experiment,
      experiment,
      repo: 'r',
      baseSha: 'a',
      prompt: 'p',
      stage: 'complete',
      createdAt: 0,
      finishedAt: 1,
      durationMs: 1,
      baseGateOk: null,
      recommended: null,
      recommendationNote: null,
      adoptedTrial: null,
      judgeCostApi: 0,
      error: null,
      trials: [],
      comparisons: [],
      rankings: [],
    });
    expect(filterRuns([run('a'), run('b')], 'b')).toHaveLength(1);
  });
});
