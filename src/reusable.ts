// Reusable worktrees: ones that take one piece of work after another, where others are kept for
// one piece of work each (reusable.rs; board.rs `reuse_of` says what one holds). What each state
// says, which worktree new work goes to in a repo (quick.ts `startLaneIn`), and when Jev's pick
// of a worktree joins the work there (route.ts).

import type { Lane, Reuse } from './types';

/** What a reusable worktree holds, in a sentence for its mark's tooltip. */
export const REUSE_WORDS: Record<Reuse, string> = {
  free: 'Ready for new work: nothing uncommitted or unlanded, and no agent at work.',
  open: 'Work under way. New work that belongs with it can join it.',
  review: 'Its PR is in review.',
  landed: 'Its work landed. New work can start here once it’s back on the default branch.',
};

/** What a reusable worktree holds, in a few words beside its name: Settings › Projects, the quick
 *  panel's worktree menu. */
export const REUSE_STATE: Record<Reuse, string> = { free: 'Ready for new work', open: 'Work under way', review: 'In review', landed: 'Landed' };

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
 * Where new work goes in the repo of `picked`, starting from it:
 * - `picked`, when it's a reusable worktree that holds no work;
 * - else the repo's reusable worktree that holds no work: free before landed (which waits on its
 *   reset), then the latest active;
 * - with none free, the main worktree, where work starts when there's nowhere else, if it's
 *   reusable; then the repo's latest active reusable worktree.
 * Null when the repo has no reusable worktree at all.
 */
export function newWorkIn(picked: Lane, lanes: Lane[]): Lane | null {
  if (takesNewWork(picked)) return picked;
  const repo = lanes.filter((l) => l.reuse && sameRepo(l, picked)).sort((a, b) => b.lastActivity - a.lastActivity);
  const order = (l: Lane) => (l.reuse === 'free' ? 0 : 1);
  return repo.filter(takesNewWork).sort((a, b) => order(a) - order(b))[0] ?? repo.find((l) => l.origin === 'main') ?? repo[0] ?? null;
}

/** How sure Jev must be of one worktree for new work to join the work there, rather than start
 *  where new work starts in its repo: new work is new work in a repo far more often than it
 *  joins a worktree's (asked 2026-11-17), and less sure, Jev matched the repo more than the
 *  work. */
export const JOIN_AT = 0.75;

/** Whether new work Jev is sure belongs in a worktree joins the work there (route.ts): the
 *  worktree isn't the main checkout, where any of the repo's work may be under way, so that it's
 *  the repo rather than the work, and it holds work under way. A reusable one says so by what it
 *  holds; one kept for its own piece of work holds it until its PR merges or closes, while its PR
 *  is open, or it has changes uncommitted or not on the default branch, as board.rs `reuse_of`
 *  reads a reusable one's. */
export function joinsWork(lane: Lane): boolean {
  if (lane.origin === 'main' || lane.startsChats) return false;
  if (lane.reuse) return lane.reuse === 'open' || lane.reuse === 'review';
  const pr = lane.pr?.state;
  if (pr === 'merged' || pr === 'closed') return false;
  return !!pr || (lane.git?.dirty ?? 0) > 0 || (lane.git?.baseAhead ?? 0) > 0;
}
