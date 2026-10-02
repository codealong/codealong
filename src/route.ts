// What the quick panel suggests from Jev's answer (route.rs): at most three rows above the
// search results, the likeliest first, so ⏎ does it. Jev gives probabilities; how sure a row
// sounds, and which runners-up are worth a row, is decided here. Kept free of React so the rules
// have tests.

import { agoPhrase } from './format';
import { homeOf, sessionWhy } from './lanes';
import { mainCheckoutOf, newWorkIn } from './reusable';
import type { Board, Lane, Route, RoutePick, Session, StartPlace } from './types';

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
 *
 * New work starts only in a reusable worktree (reusable.ts `newWorkIn`): Jev matches it to a
 * worktree of the repo it's about, and it goes to one of the repo's reusable worktrees that holds
 * no work, or joins the work under way in the one Jev picked when Jev is sure of it. A new
 * worktree only where you often start in one (route.rs `with_habit`), or when the repo has no
 * reusable worktree at all.
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
  /** Starting new work from Jev's worktree pick, `actionP` as likely as the action. In the very
   *  worktree Jev picked, it's as likely as that worktree; one chosen for it in the repo, or a new
   *  worktree, as likely as the repo. */
  const startIn = (pick: RoutePick | undefined, actionP: number): Suggestion | null => {
    const picked = laneOf(pick);
    if (!picked || !pick) return null;
    const reusable = newWorkIn(picked, pick.p, board.lanes, route.lanes);
    const newWorktree = route.ownWorktree >= OWN_WORKTREE || !reusable;
    // A new worktree is made from the repo's main worktree, so two of its worktrees make the
    // same row.
    const lane = newWorktree ? (mainCheckoutOf(picked, board.lanes) ?? picked) : (reusable ?? picked);
    const repoP = route.repos.find((r) => r.id === `${picked.project}/${picked.repo}`)?.p;
    const p = actionP * (!newWorktree && lane.id === picked.id ? pick.p : (repoP ?? pick.p));
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
      if (prob('start') >= RUNNER_UP || route.noSession >= RUNNER_UP) add(startIn(l1, Math.max(prob('start'), route.noSession)));
      break;
    }
    case 'start': {
      const p = prob('start');
      add(startIn(l1, p));
      // The session that may already be doing it comes next: sending there may be what's meant.
      // Its likelihood is that of sending at all, times that of this session doing the work, so
      // it reads on the same scale as the rows around it (the start row carries the warning).
      if (route.duplicate) add(toSession(route.duplicate, prob('send') * route.duplicate.p, false));
      if (sessionP(l2) >= RUNNER_UP) add(startIn(l2, p));
      if (prob('send') >= RUNNER_UP && sessionP(s1) >= RUNNER_UP) add(toSession(s1, prob('send') * sessionP(s1), false));
      break;
    }
    case 'todo': {
      const lane = laneOf(l1);
      if (lane) add({ kind: 'todo', key: `todo:${lane.id}`, lane, p: prob('todo') * sessionP(l1) });
      add(startIn(l1, prob('start')));
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

/** What the start row adds when Jev thinks a session may already be doing what's typed. */
export function alreadyOnWords(duplicate: { lane: Pick<Lane, 'callsign'>; session: Pick<Session, 'title' | 'account'> } | null): string {
  return duplicate ? ` “${duplicate.session.title}” (${duplicate.session.account} · ${duplicate.lane.callsign}) may already be doing this.` : '';
}

/** What a suggestion's row says goes on around it: how a new session would start, and what's typed. */
export interface SuggestionContext {
  now: number;
  /** What's typed, which a todo saved from it is titled by. */
  text: string;
  /** The account a session would start on, with what it starts on (start.ts `onModelWords`),
   *  the place it opens in, and whether its harness makes a worktree for it. */
  account: string | null;
  onModel: string;
  place: StartPlace | null;
  makesWorktree: boolean;
  /** The session that may already be doing it (`duplicateOf`). */
  duplicate: { session: Pick<Session, 'title'> } | null;
}

/** A suggestion's row: what ⏎ does, why, and where. */
export function suggestionWords(sg: Suggestion, c: SuggestionContext): { icon: string; title: string; why: string; where: string } {
  if (sg.kind === 'send' || sg.kind === 'open') {
    const x = sg.session;
    const where = `${x.account} · ${sg.lane.callsign}`;
    // How old it is leads, so an old session isn't mistaken for the one you started this morning.
    const age = x.startedAt != null ? `Started ${agoPhrase(x.startedAt, c.now)}. ` : '';
    if (sg.kind === 'open') {
      return x.status === 'needs'
        ? { icon: 'external', title: `Answer “${x.title}”`, why: `${age}It’s waiting on you, so Codealong shows it and you answer there.`, where }
        : { icon: 'external', title: `Open “${x.title}”`, why: `${age}${sessionWhy(x.detail, x.headline, c.now)}`, where };
    }
    const why = !x.live
      ? (x.harness === 'claude' || x.harness === 'codex') && x.surface === 'cli'
        ? 'It isn’t running: it resumes in a new terminal with this prompt.'
        : 'It isn’t running: it opens with this prompt ready to send.'
      : x.status === 'working'
        ? 'It’s working, so this waits for its turn to end.'
        : x.harness === 'claude' && x.surface === 'app'
          ? 'It gets this as a message from another session, in the Claude app.'
          : 'It gets this as its next message.';
    return { icon: 'send', title: `Send to “${x.title}”`, why: `${age}${why}`, where };
  }
  if (sg.kind === 'todo') {
    return { icon: 'list-check', title: `Save as a todo: “${todoTitle(c.text)}”`, why: `In ${sg.lane.project}, linked to ${sg.lane.callsign}. Start it when you’re ready.`, where: sg.lane.callsign };
  }
  const tree = c.makesWorktree && sg.newWorktree;
  const who = c.account ? `${c.account}${c.onModel}` : 'a session';
  const background = c.place === 'background' ? ' in the background' : '';
  const why =
    c.place === 'codealong'
      ? `Codealong runs ${who} in ${sg.lane.callsign}.`
      : c.place === 'claude-app'
        ? `The Claude app opens a new session in ${sg.lane.callsign} with this prompt filled in.`
        : c.place === 'codex-app'
          ? `The Codex app starts a new thread in ${sg.lane.callsign} on this prompt.`
          : tree
            ? `Claude Code makes a new worktree of ${sg.lane.repo} and starts ${who} there${background}.`
            : `Starts ${who} in ${sg.lane.callsign}${background}.`;
  // What the reusable worktree it starts in holds (reusable.ts): new work joins work under way
  // only in a worktree Jev matched it to, not the main worktree.
  const held = tree || !sg.lane.reuse ? '' : sg.lane.reuse === 'free' ? ' It holds no work now.' : sg.lane.reuse === 'landed' ? ' Its last work landed.' : sg.lane.origin === 'main' ? '' : ' It joins the work under way there.';
  // The warning leads: the row's line is cut short when it's long.
  const warned = c.duplicate ? `“${c.duplicate.session.title}” may already be on this. ${why}${held}` : `${why}${held}`;
  return { icon: 'plus', title: tree ? `Start in a new worktree of ${sg.lane.repo}` : `Start in ${sg.lane.origin === 'main' ? `${sg.lane.callsign} (main)` : sg.lane.callsign}`, why: warned, where: `${sg.lane.project} · ${sg.lane.repo}` };
}
