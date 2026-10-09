// Working through what waits on you, one session after another: the rules the three windows that
// show a session with a box to answer it go by (Sessions' panel in the main window, the session in
// the notch, the waiting window). The order they work through it in (`byPlace`), which key asks
// for what (`QUEUE_CHORDS`, `queueKey`), and what a key, an answer or a session leaving comes to
// given the window's list, the session shown and what's been dealt with (`keyStep`, `answered`,
// `left`). `useWaitingQueue` (useWaitingQueue.ts) runs them for a window; they're kept apart from
// it so the rules have tests.

import { historyChord } from './history';
import type { Left } from './menus';
import type { Status } from './types';

/** Sessions you answered, marked seen, closed or archived from the queue, with the time of the
 *  status each had then: one is left out until it waits on you anew. The board can take seconds
 *  to show an answer, and an allowed tool call isn't shown ahead of it. */
export type Handled = Map<string, number>;

/** What a row waits on you as, the most urgent first: a session that needs you, a PR waiting on
 *  your review, a turn that ended. The notch counts and lists them in this order too (bar.ts
 *  `NOTCH_ORDER`). */
export const WAITS = ['needs', 'reviews', 'unread'] as const;
export type Waits = (typeof WAITS)[number];

/** What a session waits on you as, by its status: a finished turn is Your turn, and the rest that
 *  wait (a call to allow, a question, an error) need you. */
export function statusWaits(status: Status): Waits {
  return status === 'unread' ? 'unread' : 'needs';
}

/** How the queue reads a window's rows. */
export interface Reads<T> {
  keyOf: (row: T) => string;
  /** When its status began. */
  sinceOf: (row: T) => number;
  waitsOf: (row: T) => Waits;
  titleOf: (row: T) => string;
  /** A session's status, for its glyph where the strip lists what's left (`Listed`); null for a
   *  PR waiting on your review, which has the PR's. */
  statusOf: (row: T) => Status | null;
  /** Where it is, said after its title there (`sessionWhere`, `reviewWhere`). */
  whereOf: (row: T) => string;
}

/** Where a session is, after its title: its worktree, then its account, which is said last. */
export function sessionWhere(callsign: string | null | undefined, account: string): string {
  return [callsign, account].filter(Boolean).join(' · ');
}

/** Where a PR waiting on your review is, after its title: its repo, and who asked. */
export function reviewWhere(repo: string, requestedBy: string | null | undefined): string {
  return [repo, requestedBy ? `asked by ${requestedBy}` : ''].filter(Boolean).join(' · ');
}

/** Where a row stands in the queue: what it waits as, since when, and its key, for a tie. */
export interface Place {
  key: string;
  waits: Waits;
  since: number;
}

export function placeOf<T>(row: T, reads: Pick<Reads<T>, 'keyOf' | 'sinceOf' | 'waitsOf'>): Place {
  return { key: reads.keyOf(row), waits: reads.waitsOf(row), since: reads.sinceOf(row) };
}

/**
 * The order every window works through what waits on you in: the most urgent kind first
 * (`WAITS`), each first come, first served, the longest wait first. One that comes to wait, or
 * comes back after you answered it, joins the end of its kind and never goes ahead of the ones
 * that have waited longer.
 */
export function byPlace(a: Place, b: Place): number {
  return WAITS.indexOf(a.waits) - WAITS.indexOf(b.waits) || a.since - b.since || a.key.localeCompare(b.key);
}

/** A window's rows in the queue's order (`byPlace`), whatever order it lists them in. */
export function inQueueOrder<T>(rows: T[], reads: Pick<Reads<T>, 'keyOf' | 'sinceOf' | 'waitsOf'>): T[] {
  return rows
    .map((row) => ({ row, at: placeOf(row, reads) }))
    .sort((a, b) => byPlace(a.at, b.at))
    .map((r) => r.row);
}

/**
 * What's left to answer after the session `key`: those after it in the queue's order, then those
 * before it (the ones you skipped, and any that came to wait ahead of where you are), without it
 * and the ones you've dealt with. The first is the one a window shows next. One the queue no
 * longer lists goes on from where it stood (`was`, while that's its own: answered, the board can
 * show it working before the answer's in); one that never waited goes from the top.
 */
export function leftAfter<T>(queue: T[], key: string, handled: Handled, reads: Pick<Reads<T>, 'keyOf' | 'sinceOf' | 'waitsOf'>, was?: Place | null): T[] {
  const rows = inQueueOrder(queue, reads).map((row) => ({ row, at: placeOf(row, reads) }));
  const from = rows.find((r) => r.at.key === key)?.at ?? (was?.key === key ? was : null);
  const i = from ? rows.findIndex((r) => byPlace(r.at, from) > 0) : 0;
  const order = i < 0 ? rows : [...rows.slice(i), ...rows.slice(0, i)];
  return order
    .filter(({ at }) => {
      const dealt = handled.get(at.key);
      return at.key !== key && (dealt === undefined || at.since > dealt);
    })
    .map((r) => r.row);
}

/**
 * What a key asks for while a session shows: `next` skips to the next that waits on you (it comes
 * round again after the rest) and `back` goes to the one before; `seen` is ⌘E, done with this
 * one (`seenStep`); `down` and `up` step along everything the window lists, the running ones
 * too; `historyBack` and `historyForward` go through the sessions looked at (history.ts).
 * ⌘⏎, which answers and goes on, is the box's own (SessionReply.tsx) and reports what it did.
 */
export type QueueAction = 'next' | 'back' | 'seen' | 'down' | 'up' | 'historyBack' | 'historyForward';

/** Each action's keys, written as a menu's accelerators are (menus.ts `chordSigns` says them as
 *  the keys' signs). The first is the one a button says. The windows' handlers and the strip over
 *  a session's header both read them here, so a key changed here changes in every window. */
export const QUEUE_CHORDS: Record<QueueAction, readonly string[]> = {
  next: ['Cmd+]', 'Alt+Shift+Down'],
  back: ['Cmd+[', 'Alt+Shift+Up'],
  seen: ['Cmd+E'],
  down: ['Alt+Down'],
  up: ['Alt+Up'],
  historyBack: ['Ctrl+-'],
  historyForward: ['Ctrl+Shift+-'],
};

/** Escape puts away a session that can be put away (the notch's). It's each window's own key,
 *  not a step through the queue; the strip's × says it from here. */
export const AWAY_CHORD = 'Esc';

type Key = Pick<KeyboardEvent, 'key' | 'code' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>;

/** Whether a key event is the chord, with exactly its modifiers. */
export function chordPressed(chord: string, e: Key): boolean {
  const parts = chord.split('+');
  const key = parts.pop() ?? '';
  const has = (mod: string) => parts.includes(mod);
  if (e.metaKey !== has('Cmd') || e.ctrlKey !== has('Ctrl') || e.shiftKey !== has('Shift')) return false;
  // ] and [ go by their keys' codes, which layouts that type them with ⌥ keep, and with ⌥ held or
  // not, as the main window and the quick panel have always taken ].
  if (key === ']') return e.code === 'BracketRight';
  if (key === '[') return e.code === 'BracketLeft';
  if (e.altKey !== has('Alt')) return false;
  // - goes by what it types or where it is, as history.ts has it.
  if (key === '-') return historyChord(e);
  // ` goes by what it types or where it is too: layouts that make it a dead key type nothing.
  if (key === '`') return e.key === '`' || e.code === 'Backquote';
  if (key === 'Up' || key === 'Down') return e.key === `Arrow${key}`;
  if (key === 'Enter') return e.key === 'Enter';
  return e.key.toUpperCase() === key;
}

/** The action a key event asks for, or none. */
export function queueKey(e: Key): QueueAction | null {
  const actions = Object.keys(QUEUE_CHORDS) as QueueAction[];
  return actions.find((a) => QUEUE_CHORDS[a].some((chord) => chordPressed(chord, e))) ?? null;
}

/** Whether the session `key` is one of those waiting on you, dealt with or not. */
export function waits<T>(waiting: T[], key: string, keyOf: (row: T) => string): boolean {
  return waiting.some((r) => keyOf(r) === key);
}

/** Where `next` and `back` go from the session `key`: the first of what's left after it, or the
 *  last, which is the one before it. None with nothing else waiting. From a session that doesn't
 *  wait itself, `next` is the first that does. */
export function stepTo<T>(action: 'next' | 'back', view: Pick<QueueView<T>, 'waiting' | 'reads' | 'was'>, key: string, handled: Handled): T | null {
  const left = leftAfter(view.waiting, key, handled, view.reads, view.was);
  return (action === 'next' ? left[0] : left[left.length - 1]) ?? null;
}

/** The row `delta` along `rows` from the session `key`, stopping at the ends; the first, or the
 *  last going back, from one the rows don't list. None with no rows. */
export function stepAlong<T>(rows: T[], key: string, delta: 1 | -1, keyOf: (row: T) => string): T | null {
  if (rows.length === 0) return null;
  const i = rows.findIndex((r) => keyOf(r) === key);
  if (i < 0) return delta > 0 ? rows[0] : rows[rows.length - 1];
  return rows[Math.min(rows.length - 1, Math.max(0, i + delta))];
}

/** What ⌘E comes to on the session shown. */
export type SeenStep = 'seen' | 'skip' | 'stay';

/**
 * ⌘E is done with this one. A finished turn, or a reply that asks you something (`seenable`), is
 * marked seen, and so leaves what waits on you (`leftGoesOn`). With nothing to mark seen, a
 * session that doesn't wait on you (it runs, say) is skipped as ⌘] skips, and one that waits on a
 * call to allow stays: ⌘E doesn't answer it.
 */
export function seenStep(seenable: boolean, waiting: boolean): SeenStep {
  if (seenable) return 'seen';
  return waiting ? 'stay' : 'skip';
}

/**
 * Whether a window goes on to the next when the session shown left what waits on you (marked
 * seen, snoozed, closed, archived). From one that waited on you, always. One that didn't wait
 * goes on too where the window shows only the session (the notch, the waiting window:
 * it has left them), and stays where it's a row in a list (`stays`: archiving down Sessions' old
 * sessions doesn't jump to one that needs you).
 */
export function leftGoesOn(waiting: boolean, stays: boolean): boolean {
  return waiting || !stays;
}

/** How the last one went, as the strip says it on the one gone on to. */
export function wentWords(title: string, done: string): string {
  return `${title}: ${done}`;
}

/** The session a window shows, as the rules need it. */
export interface Shown {
  key: string;
  title: string;
  /** When its status began; left out where the window's list no longer has it, and it's taken as
   *  dealt with now. */
  since?: number;
  /** It's a finished turn, or a reply that asks you something: ⌘E marks it seen. */
  seenable: boolean;
  /** It waits on you with nothing to mark seen and nothing ⌘E could answer (a PR waiting on
   *  your review): ⌘E skips it, as it skips one running, rather than staying as it does on a
   *  call to allow. */
  skipsOnSeen?: boolean;
}

/** A window as the rules see it: its lists, the session it shows, and what's its own to do. */
export interface QueueView<T> {
  /** Its sessions that wait on you, snoozed ones left out, in any order: the rules go through
   *  them in the queue's (`byPlace`). */
  waiting: T[];
  reads: Reads<T>;
  /** The session shown, or null with none. */
  shown: Shown | null;
  /** Where the session shown stood the last time `waiting` listed it. Answered, it can leave the
   *  list before the answer's in, and the window goes on from there rather than from the top. */
  was?: Place | null;
  /** Everything ⌥↑ ⌥↓ step along, the running ones too. Without it those keys are the window's
   *  own: the main window's arrows move along its list in every view, not only in Sessions. */
  along?: T[];
  /** It has a history of the sessions looked at for the queue's keys to go through. Without one
   *  ⌃- and ⌃⇧- are the window's own: the main window's work from every view and field. */
  travels?: boolean;
  /** ⌘E is the window's to run (the main window, whose lists have the key in every view). Where
   *  a card shows the session, ⌘E on one that can be marked seen is the card's (SessionCard.tsx),
   *  which marks it seen and says it left. */
  marksSeen?: boolean;
  /** A session that left what waits on you without having waited stays as it is (`leftGoesOn`):
   *  the main window, where it's a row in a list. */
  leftStays?: boolean;
}

/** What a step came to: `on` to another session; `last` when there's nowhere to go (nothing else
 *  waits, or it's the end of the list); `stay` when it isn't a step at all (a session that left
 *  without having waited, where it stays as it is; ⌘E on a call to allow); `away` when the session
 *  it was done to is no longer the one shown, and the window stays where you put it. */
export type Went = 'on' | 'last' | 'stay' | 'away';

export interface Step<T> {
  went: Went;
  /** The row that takes the window, when it went on. */
  to?: T;
  /** What the strip says on it: how the last one went, or null after a step by hand, which takes
   *  back what it said. */
  words?: string | null;
  /** What's been dealt with, after the step. */
  handled: Handled;
}

/** What a key comes to. */
export interface KeyStep<T> extends Step<T> {
  /** The key was the queue's. False when it's the window's own or the card's, or nothing shows:
   *  the event goes on to whoever's it is. */
  taken: boolean;
  /** The session to mark seen, before the window goes on. */
  markSeen?: string;
  /** Back or forward through the window's history, which says where that is. */
  travel?: 'back' | 'forward';
}

/** The session shown is dealt with: the next that waits takes its place, saying what was done. */
function dealt<T>(view: QueueView<T>, handled: Handled, s: Shown, done: string, now: number): Step<T> {
  const after = new Map(handled).set(s.key, s.since ?? now);
  const to = stepTo('next', view, s.key, after);
  return to ? { went: 'on', to, words: wentWords(s.title, done), handled: after } : { went: 'last', handled: after };
}

/**
 * ⌘⏎ sent the session `from` a message, allowed its call or answered its questions: it's dealt
 * with, and the next that waits on you takes its place, the strip saying what was done. An
 * answer that comes in after you've moved on by hand leaves the window where you put it, and the
 * session as it was.
 */
export function answered<T>(view: QueueView<T>, handled: Handled, from: string, done: string, now: number): Step<T> {
  const s = view.shown;
  return s && s.key === from ? dealt(view, handled, s, done, now) : { went: 'away', handled };
}

/**
 * The session `from` left what waits on you: marked seen, snoozed, closed or archived. The window
 * goes on as `leftGoesOn` says. A snoozed one isn't kept as dealt with: the snooze shows at once
 * and ends by itself, and when its time comes the session waits on you again, with the status it
 * had.
 */
export function left<T>(view: QueueView<T>, handled: Handled, from: string, done: Left, now: number): Step<T> {
  const s = view.shown;
  if (!s || s.key !== from) return { went: 'away', handled };
  if (!leftGoesOn(waits(view.waiting, s.key, view.reads.keyOf), !!view.leftStays)) return { went: 'stay', handled };
  const step = dealt(view, handled, s, `${done}.`, now);
  if (done === 'snoozed') step.handled.delete(s.key);
  return step;
}

/** What a key's action comes to on the session shown. */
export function keyStep<T>(view: QueueView<T>, handled: Handled, action: QueueAction, now: number): KeyStep<T> {
  const s = view.shown;
  const no: KeyStep<T> = { taken: false, went: 'stay', handled };
  if (!s) return no;
  /** A step by hand: nothing is dealt with, and the strip takes back what it said. */
  const hand = (to: T | null): KeyStep<T> => (to ? { taken: true, went: 'on', to, words: null, handled } : { taken: true, went: 'last', handled });
  if (action === 'next' || action === 'back') return hand(stepTo(action, view, s.key, handled));
  if (action === 'seen') {
    const does = s.skipsOnSeen ? 'skip' : seenStep(s.seenable, waits(view.waiting, s.key, view.reads.keyOf));
    if (does === 'skip') return hand(stepTo('next', view, s.key, handled));
    if (does === 'stay' || !view.marksSeen) return no;
    return { ...left(view, handled, s.key, 'marked seen', now), taken: true, markSeen: s.key };
  }
  if (action === 'down' || action === 'up') {
    if (!view.along) return no;
    const to = stepAlong(view.along, s.key, action === 'down' ? 1 : -1, view.reads.keyOf);
    return hand(to && view.reads.keyOf(to) !== s.key ? to : null);
  }
  return view.travels ? { taken: true, went: 'stay', handled, travel: action === 'historyBack' ? 'back' : 'forward' } : no;
}

/** The first that waits and hasn't been dealt with, for a window that shows nothing: the waiting
 *  window, after the last one, goes to the next that comes to wait on you, and not to one you
 *  just answered that the board still shows waiting. */
export function firstLeft<T>(view: Pick<QueueView<T>, 'waiting' | 'reads'>, handled: Handled): T | undefined {
  return leftAfter(view.waiting, '', handled, view.reads)[0];
}

/** One of what's left as the strip lists it when you point at its count (SessionPanel.tsx
 *  `QueueStrip`), as a folded pill on the bar lists its sessions. */
export interface Listed {
  key: string;
  waits: Waits;
  title: string;
  status: Status | null;
  where: string;
}

export function listed<T>(rows: T[], reads: Reads<T>): Listed[] {
  return rows.map((r) => ({ key: reads.keyOf(r), waits: reads.waitsOf(r), title: reads.titleOf(r), status: reads.statusOf(r), where: reads.whereOf(r) }));
}

/** One picked from the strip's list: a step by hand to it, as ⌘] is, so nothing is dealt with
 *  and the strip takes back what it said. Nowhere when it no longer waits, or is the one shown. */
export function pickStep<T>(view: Pick<QueueView<T>, 'waiting' | 'reads' | 'shown'>, handled: Handled, key: string): Step<T> {
  const to = key === view.shown?.key ? undefined : view.waiting.find((r) => view.reads.keyOf(r) === key);
  return to !== undefined ? { went: 'on', to, words: null, handled } : { went: 'stay', handled };
}

/** How many of each kind wait on you, as the notch counts them. */
export type WaitCounts = Record<Waits, number>;

export function waitCounts<T>(rows: T[], waitsOf: (row: T) => Waits): WaitCounts {
  const counts: WaitCounts = { needs: 0, reviews: 0, unread: 0 };
  for (const r of rows) counts[waitsOf(r)] += 1;
  return counts;
}

/** A kind's count in words, for a tooltip: `2 need you`, `1 PR to review`, `3 your turn`. */
export function waitCountWords(waits: Waits, n: number): string {
  if (waits === 'needs') return `${n} ${n === 1 ? 'needs' : 'need'} you`;
  if (waits === 'reviews') return `${n} ${n === 1 ? 'PR' : 'PRs'} to review`;
  return `${n} your turn`;
}

/** What the strip over the header goes by: what's left after the session shown, the next first,
 *  and whether it's one of those waiting itself. */
export function standing<T>(view: QueueView<T>, handled: Handled): { left: T[]; waiting: boolean } | null {
  const s = view.shown;
  return s ? { left: leftAfter(view.waiting, s.key, handled, view.reads, view.was), waiting: waits(view.waiting, s.key, view.reads.keyOf) } : null;
}
