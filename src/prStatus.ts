// Where a PR stands, as the PRs view and the PR panel show it: a pill for its review, one for
// its checks, and one for anything else that keeps GitHub from merging it. When nothing does,
// they fold into a single "Ready to merge". The PRs view also flags the rows that are your move.

import type { Checks, Merge, PrItem, PrState } from './types';

/** A pill's color: green for approved and passing, amber for what needs you, red for failing. */
export type PrTone = 'ready' | 'pass' | 'attn' | 'alert' | 'plain' | 'quiet' | 'merged';

export interface PrPill {
  key: 'ready' | 'review' | 'checks' | 'merge' | 'state';
  tone: PrTone;
  icon: string | null;
  text: string;
  title: string;
}

/** What the pills read: a worktree's PR or a PRs-view item. */
export interface PrFacts {
  state: PrState;
  checks: Checks;
  merge: Merge;
  base?: string | null;
}

const isOpen = (state: PrState) => state !== 'merged' && state !== 'closed';

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;

/** GitHub would merge it now: the reviews the repo requires, passing checks and no conflicts.
 *  Only GitHub can say so; approved with passing checks can still conflict with its base. */
export function prReady(pr: PrFacts): boolean {
  return isOpen(pr.state) && pr.state !== 'draft' && pr.merge === 'clean' && pr.checks.failed === 0 && pr.checks.pending === 0;
}

function reviewPill(pr: PrFacts): PrPill {
  const pill = (tone: PrTone, text: string, title: string): PrPill => ({ key: 'review', tone, icon: null, text, title });
  switch (pr.state) {
    case 'draft':
      return pill('quiet', 'Draft', 'A draft: not ready for review yet');
    case 'changes':
      return pill('attn', 'Changes requested', 'A reviewer asked for changes');
    case 'approved':
      if (pr.merge === 'blocked') return pill('pass', 'Approved', 'Approved, but something the repo requires still holds back the merge');
      if (pr.merge === 'unknown') return pill('pass', 'Approved', "Approved. GitHub hasn't said yet whether it can merge");
      return pill('pass', 'Approved', 'Approved');
    default:
      return pill('plain', 'Awaiting review', 'Not approved yet');
  }
}

function checksPill(c: Checks): PrPill | null {
  if (c.failed > 0) {
    const names = c.failing.length > 0 ? `: ${c.failing.join(', ')}` : '';
    return { key: 'checks', tone: 'alert', icon: 'x', text: `${c.failed} failing`, title: `${plural(c.failed, 'check')} failing${names}` };
  }
  if (c.pending > 0) {
    const passed = c.passed > 0 ? `, ${c.passed} passed` : '';
    return { key: 'checks', tone: 'plain', icon: 'circle-dashed', text: `${c.pending} running`, title: `${plural(c.pending, 'check')} running${passed}` };
  }
  if (c.passed > 0) return { key: 'checks', tone: 'pass', icon: 'check', text: `${c.passed}`, title: `${plural(c.passed, 'check')} passed` };
  return null;
}

/** The pills for a PR, in a fixed order so they line up down a list: review, checks, merge. */
export function prPills(pr: PrFacts): PrPill[] {
  if (pr.state === 'merged') return [{ key: 'state', tone: 'merged', icon: null, text: 'Merged', title: 'Merged' }];
  if (pr.state === 'closed') return [{ key: 'state', tone: 'quiet', icon: null, text: 'Closed', title: 'Closed without merging' }];
  const c = pr.checks;
  if (prReady(pr)) {
    const why = [pr.state === 'approved' ? 'Approved' : null, c.passed > 0 ? `${plural(c.passed, 'check')} passed` : null, 'no conflicts'].filter(Boolean).join(', ');
    return [{ key: 'ready', tone: 'ready', icon: 'merge', text: 'Ready to merge', title: `${why[0].toUpperCase()}${why.slice(1)}: GitHub can merge it` }];
  }
  const base = pr.base ?? 'its base';
  const pills = [reviewPill(pr), checksPill(c)].filter((p): p is PrPill => p !== null);
  if (pr.merge === 'conflicts') {
    pills.push({ key: 'merge', tone: 'attn', icon: 'alert', text: 'Conflicts', title: `Conflicts with ${base}: resolve them before it can merge` });
  } else if (pr.merge === 'behind') {
    pills.push({ key: 'merge', tone: 'attn', icon: null, text: 'Out of date', title: `Behind ${base}, and the repo wants it up to date before it merges` });
  }
  return pills;
}

/** Why a PR is your move, if it is: your review is asked for, or it's yours and GitHub would
 *  merge it, or its checks fail, or it's held up by something you can fix. `attn` and `alert`
 *  are what the Needs you filter keeps. */
export function prFlag(pr: PrItem): 'ready' | 'attn' | 'alert' | null {
  if (!isOpen(pr.state)) return null;
  if (pr.reviewRequested) return 'attn';
  if (!pr.mine) return null;
  if (prReady(pr)) return 'ready';
  if (pr.checks.failed > 0) return 'alert';
  if (pr.state === 'changes' || pr.merge === 'conflicts' || pr.merge === 'behind') return 'attn';
  return null;
}
