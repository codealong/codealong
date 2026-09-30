// What the quick panel suggests from Jev's answer (route.rs): at most three rows above the
// search results, the likeliest first, so ⏎ does it. Jev gives probabilities; how sure a row
// sounds, and which runners-up are worth a row, is decided here. Kept free of React so the rules
// have tests.

import { homeOf } from './lanes';
import type { Board, Lane, Route, RoutePick, Session } from './types';

/** How likely a suggestion is, as the panel says it. */
export type Sureness = 'sure' | 'likely' | 'unsure';

export type Suggestion =
  | { kind: 'send'; key: string; lane: Lane; session: Session; p: number }
  | { kind: 'open'; key: string; lane: Lane; session: Session; p: number }
  | { kind: 'start'; key: string; lane: Lane; newWorktree: boolean; p: number }
  | { kind: 'todo'; key: string; lane: Lane; p: number };

/** Rows at most. */
const ROWS = 3;
/** A runner-up gets a row when it's at least this likely. */
const RUNNER_UP = 0.15;
/** New work gets its own worktree when Jev is at least this sure it wants one. */
const OWN_WORKTREE = 0.6;
/** Typed text shorter than this is a search, not worth asking about. */
export const MIN_ROUTE_CHARS = 8;

export function sureness(p: number): Sureness {
  return p >= 0.75 ? 'sure' : p >= 0.45 ? 'likely' : 'unsure';
}

/** Whether what's typed is worth asking Jev about: a few words, not a word or two of a search. */
export function worthRouting(text: string): boolean {
  const t = text.trim();
  return t.length >= MIN_ROUTE_CHARS && t.split(/\s+/).length >= 2;
}

function findSession(board: Board, key: string): { lane: Lane; session: Session } | null {
  return homeOf(board.lanes, key);
}

/**
 * The rows for an answer. The likeliest action on its likeliest target leads, sureness from
 * both probabilities multiplied. Runners-up follow: another likely session, or starting new
 * work when that's likely too, so a wrong guess is one ↓ away from the right one. A session that
 * waits on you for a prompt is opened, never sent to: what's typed would answer the prompt. One
 * whose reply asks you something is sent to: what's typed is its answer.
 */
export function suggestions(route: Route, board: Board): Suggestion[] {
  const prob = (a: string) => route.actions.find(([x]) => x === a)?.[1] ?? 0;
  const laneOf = (pick: RoutePick | undefined) => (pick ? (board.lanes.find((l) => l.id === pick.id) ?? null) : null);
  const out: Suggestion[] = [];
  const seen = new Set<string>();
  const add = (s: Suggestion | null) => {
    if (!s || seen.has(s.key) || out.length >= ROWS) return;
    seen.add(s.key);
    out.push(s);
  };

  const toSession = (pick: RoutePick | undefined, p: number, find: boolean): Suggestion | null => {
    const found = pick && findSession(board, pick.id);
    if (!found) return null;
    const open = find || (found.session.status === 'needs' && !found.session.asks);
    return { kind: open ? 'open' : 'send', key: `${open ? 'open' : 'send'}:${found.session.key}`, ...found, p };
  };
  const startIn = (pick: RoutePick | undefined, p: number): Suggestion | null => {
    const lane = laneOf(pick);
    if (!lane) return null;
    const newWorktree = route.ownWorktree >= OWN_WORKTREE;
    // A new worktree is made from the repo, so two of its worktrees make the same row.
    return { kind: 'start', key: newWorktree ? `start:new:${lane.project}/${lane.repo}` : `start:${lane.id}`, lane, newWorktree, p };
  };
  const [s1, s2] = route.sessions;
  const [l1, l2] = route.lanes;
  const sessionP = (s: RoutePick | undefined) => s?.p ?? 0;

  switch (route.action) {
    case 'send':
    case 'find': {
      const find = route.action === 'find';
      const p = prob(route.action);
      add(toSession(s1, p * sessionP(s1), find));
      if (sessionP(s2) >= RUNNER_UP) add(toSession(s2, p * sessionP(s2), find));
      if (prob('start') >= RUNNER_UP || route.noSession >= RUNNER_UP) add(startIn(l1, Math.max(prob('start'), route.noSession) * sessionP(l1)));
      break;
    }
    case 'start': {
      const p = prob('start');
      add(startIn(l1, p * sessionP(l1)));
      // The session that may already be doing it comes next: sending there may be what's meant.
      // Its likelihood is that of sending at all, times that of this session doing the work, so
      // it reads on the same scale as the rows around it (the start row carries the warning).
      if (route.duplicate) add(toSession(route.duplicate, prob('send') * route.duplicate.p, false));
      if (sessionP(l2) >= RUNNER_UP) add(startIn(l2, p * sessionP(l2)));
      if (prob('send') >= RUNNER_UP && sessionP(s1) >= RUNNER_UP) add(toSession(s1, prob('send') * sessionP(s1), false));
      break;
    }
    case 'todo': {
      const lane = laneOf(l1);
      if (lane) add({ kind: 'todo', key: `todo:${lane.id}`, lane, p: prob('todo') * sessionP(l1) });
      add(startIn(l1, prob('start') * sessionP(l1)));
      break;
    }
  }
  return out;
}

/** The session that may already be doing what's typed, when Jev thinks one is. */
export function duplicateOf(route: Route | null, board: Board): { lane: Lane; session: Session } | null {
  return route?.duplicate ? findSession(board, route.duplicate.id) : null;
}

/** Sessions found by the typed words, with the ones Jev scored first, in its order; the rest
 *  keep theirs. */
export function rankedBy<T extends { session: Session }>(found: T[], route: Route | null): T[] {
  if (!route) return found;
  const rank = new Map(route.sessions.map((s, i) => [s.id, i]));
  const at = (r: T) => rank.get(r.session.key) ?? Infinity;
  return found
    .map((r, i) => ({ r, i }))
    .sort((a, b) => at(a.r) - at(b.r) || a.i - b.i)
    .map((x) => x.r);
}

/** A todo's title from what's typed: its first line, without a "todo:" or "later," lead-in. */
export function todoTitle(text: string): string {
  const first = text.trim().split('\n')[0].trim();
  const stripped = first.replace(/^(todo|later|remember to|note)\b[\s:,-]*/i, '').trim();
  const title = stripped || first;
  return title.charAt(0).toUpperCase() + title.slice(1);
}
