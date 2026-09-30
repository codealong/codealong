// Reusable worktrees: ones that take one piece of work after another, where others are kept for
// one piece of work each (reusable.rs; board.rs `reuse_of` says what one holds). What each state
// says, and which worktree new work goes to when Jev picks one that isn't reusable (route.ts).

import type { Lane, Reuse, RoutePick } from './types';

/** What a reusable worktree holds, in a sentence for its mark's tooltip. */
export const REUSE_WORDS: Record<Reuse, string> = {
  free: 'Free: nothing uncommitted or unlanded, and no agent at work. New work can start here.',
  open: 'Work under way. New work that belongs with it can join it.',
  review: 'Its PR is in review.',
  landed: 'Its work landed. New work can start here once it’s back on the default branch.',
};

/** Whether new work can start in a worktree without joining other work: reusable, and holding
 *  none. */
export function takesNewWork(lane: Lane): boolean {
  return lane.reuse === 'free' || lane.reuse === 'landed';
}

/** Whether two worktrees are of one repo on one device. */
function sameRepo(a: Lane, b: Lane): boolean {
  return a.project === b.project && a.repo === b.repo && (a.device?.short ?? null) === (b.device?.short ?? null);
}

/**
 * Where new work Jev placed in a worktree kept for its own work goes instead: a reusable worktree
 * of the same repo that holds no work, the one Jev ranked highest first, then free before landed
 * (which waits on its reset), then the latest active. Null when the repo has none free, and the
 * work wants a new worktree.
 */
export function reusableFor(lane: Lane, lanes: Lane[], ranked: RoutePick[]): Lane | null {
  const rank = (l: Lane) => {
    const i = ranked.findIndex((p) => p.id === l.id);
    return i < 0 ? Infinity : i;
  };
  const order = (l: Lane) => (l.reuse === 'free' ? 0 : 1);
  const open = lanes.filter((l) => l.id !== lane.id && sameRepo(l, lane) && takesNewWork(l));
  open.sort((a, b) => rank(a) - rank(b) || order(a) - order(b) || b.lastActivity - a.lastActivity);
  return open[0] ?? null;
}

/** The repo's main checkout, where a new worktree is made from: Claude Code makes one under the
 *  checkout it starts in. */
export function mainCheckoutOf(lane: Lane, lanes: Lane[]): Lane | null {
  if (lane.origin === 'main') return lane;
  return lanes.find((l) => l.origin === 'main' && sameRepo(l, lane)) ?? null;
}
