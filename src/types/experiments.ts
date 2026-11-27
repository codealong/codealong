// Experiments report and leaderboard types (src-tauri/src/experiments/report.rs), for the
// Analytics window's Experiments tab (`ui/UsageExperiments.tsx`).

export type ExperimentRunStage = 'running' | 'judging' | 'complete' | 'cancelled' | 'unknown';
export type ExperimentTrialStage = 'queued' | 'preparing' | 'working' | 'reviewing' | 'fixing' | 'gating' | 'ready' | 'failed' | 'voided' | 'unknown';

export interface ExperimentCost {
  totalApi: number;
  workerApi: number;
  reviewerApi: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  unpricedCalls: number;
}

export interface ExperimentTrialView {
  id: string;
  label: string;
  worker: string;
  reviewer: string | null;
  observedModel: string | null;
  stage: ExperimentTrialStage;
  worktree: string | null;
  branch: string | null;
  workerKey: string | null;
  reviewerKey: string | null;
  diffBytes: number;
  diffRef: string | null;
  reviewVerdict: string | null;
  reviewerModifiedTree: boolean;
  gatePassed: boolean | null;
  prOpened: boolean;
  drift: string | null;
  error: string | null;
  interruptions: number;
  durationMs: number | null;
  cost: ExperimentCost;
  adopted: boolean;
  discarded: boolean;
  recommended: boolean;
}

export interface ExperimentPairView {
  trialA: string;
  trialB: string;
  labelA: string;
  labelB: string;
  judge: string | null;
  forwardWinner: string | null;
  reversedWinner: string | null;
  consistentWinner: string | null;
  rationale: string | null;
}

export interface ExperimentRankView {
  id: string;
  judge: string;
  ranking: string[][];
  rankingLabels: string[][];
  rationale: string | null;
  error: string | null;
  done: boolean;
}

export interface ExperimentRunView {
  id: string;
  experiment: string;
  repo: string;
  baseSha: string;
  prompt: string;
  stage: ExperimentRunStage;
  createdAt: number;
  finishedAt: number | null;
  durationMs: number | null;
  baseGateOk: boolean | null;
  recommended: string | null;
  recommendationNote: string | null;
  adoptedTrial: string | null;
  judgeCostApi: number;
  error: string | null;
  trials: ExperimentTrialView[];
  comparisons: ExperimentPairView[];
  rankings: ExperimentRankView[];
}

export interface ExperimentLeaderboardRow {
  key: string;
  experiment: string;
  workerHarness: string;
  workerModel: string;
  workerEffort: string | null;
  reviewer: string | null;
  label: string;
  runs: number;
  wins: number;
  ties: number;
  losses: number;
  gatePassed: number;
  gateTotal: number;
  avgCostApi: number;
  avgWorkerCostApi: number;
  avgReviewerCostApi: number;
  avgDurationMs: number | null;
  adoptedCount: number;
}

export interface ExperimentReport {
  on: boolean;
  experiments: string[];
  leaderboard: ExperimentLeaderboardRow[];
  runs: ExperimentRunView[];
}
