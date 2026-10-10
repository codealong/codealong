// Reusable worktrees: ones that take one piece of work after another, where others are kept for
// one piece of work each (reusable.rs; board.rs `reuse_of` says what one holds). What each state
// says, and which worktree new work goes to from the one Jev picked (route.ts).

import type { Lane, Reuse, RoutePick } from './types';

/** What a reusable worktree holds, in a sentence for its mark's tooltip. */
export const REUSE_WORDS: Record<Reuse, string> = {
  free: 'Ready for new work: nothing uncommitted or unlanded, and no agent at work.',
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

/** How sure Jev must be of a reusable worktree with work under way for new work to join that
 *  work: less sure, it matched the repo more than the work. */
export const JOIN_AT = 0.5;

/**
 * Where new work goes from the worktree Jev picked, at Jev's probability `p`:
 * - a reusable worktree that holds no work, as picked;
 * - one with work under way, joining that work, when Jev is sure of it (`JOIN_AT`) and it isn't
 *   the main worktree, which is where any of the repo's work may be under way;
 * - otherwise the repo's reusable worktree that holds no work: the one Jev ranked highest, then
 *   free before landed (which waits on its reset), then the latest active;
 * - with none free, the main worktree, where work starts when there's nowhere else, if it's
 *   reusable; then the repo's reusable worktree Jev ranked highest.
 * Null when the repo has no reusable worktree at all.
 */
export function newWorkIn(picked: Lane, p: number, lanes: Lane[], ranked: RoutePick[]): Lane | null {
  if (takesNewWork(picked)) return picked;
  if (picked.reuse && picked.origin !== 'main' && p >= JOIN_AT) return picked;
  const rank = (l: Lane) => {
    const i = ranked.findIndex((r) => r.id === l.id);
    return i < 0 ? Infinity : i;
  };
  const byRank = (a: Lane, b: Lane) => rank(a) - rank(b) || b.lastActivity - a.lastActivity;
  const repo = lanes.filter((l) => l.reuse && sameRepo(l, picked));
  const order = (l: Lane) => (l.reuse === 'free' ? 0 : 1);
  const free = repo.filter(takesNewWork).sort((a, b) => rank(a) - rank(b) || order(a) - order(b) || b.lastActivity - a.lastActivity);
  return free[0] ?? repo.find((l) => l.origin === 'main') ?? repo.sort(byRank)[0] ?? null;
}

/** The repo's main worktree, where a new worktree is made from: Claude Code makes one under the
 *  checkout it starts in. */
export function mainCheckoutOf(lane: Lane, lanes: Lane[]): Lane | null {
  if (lane.origin === 'main') return lane;
  return lanes.find((l) => l.origin === 'main' && sameRepo(l, lane)) ?? null;
}
