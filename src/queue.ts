// Working through what waits on you, one session after another: the rules the three windows that
// show a session with a box to answer it go by (Sessions' panel in the main window, the session in
// the notch, the waiting window). The order they work through it in (`byPlace`), which key asks
// for what (`QUEUE_CHORDS`, `queueKey`), and what a key, an answer or a session leaving comes to
// given the window's list, the session shown and what's been dealt with (`keyStep`, `answered`,
// `left`). `useWaitingQueue` (useWaitingQueue.ts) runs them for a window; they're kept apart from
// it so the rules have tests.

import { byRaise } from './lanes';
import type { Left } from './menus';
import { chordPressed } from './platform';
import type { Raised, Status } from './types';

/** Sessions you answered, marked seen, closed or archived from the queue, with the time of the
 *  status each had then: one is left out until it waits on you anew. The board can take seconds
 *  to show an answer, and an allowed tool call isn't shown ahead of it. */
export type Handled = Map<string, number>;

/** What a row waits on you as, in the order the queue goes to them unless you put them in
 *  another (`[queue] order`, `queueKinds`): a session that needs you, a PR waiting on your
 *  review, a PR of yours waiting on you to merge it (`PrItem.waitsToMerge`), a todo due
 *  (barfeed.rs `todo_lists`), a turn that ended, and last a todo ready to
 *  start, which the queue reaches once nothing else waits. The notch counts them in that order
 *  too (bar.ts `notchOrder`). A review whose repo puts reviews first goes ahead of them all
 *  (`Place.first`). config.rs `QUEUE_KINDS` has them too. */
export const WAITS = ['needs', 'reviews', 'merges', 'due', 'unread', 'ready'] as const;
export type Waits = (typeof WAITS)[number];

/** The kinds that always wait on you, which `[queue] order` can move but not leave out: the
 *  sessions' (config.rs `ALWAYS_WAITS`). */
export const ALWAYS_WAITS: readonly Waits[] = ['needs', 'unread'];

/** `kinds` with `kind` put where the default order has it among them: before the first that
 *  comes after it there. */
export function withKind(kinds: readonly Waits[], kind: Waits): Waits[] {
  if (kinds.includes(kind)) return [...kinds];
  const rank = (w: Waits) => WAITS.indexOf(w);
  const at = kinds.findIndex((k) => rank(k) > rank(kind));
  return at < 0 ? [...kinds, kind] : [...kinds.slice(0, at), kind, ...kinds.slice(at)];
}

// One list for each order read, so a window's rows and reads change only when the order does.
const KINDS = new Map<string, readonly Waits[]>();

/**
 * What waits on you in your order (`[queue] order`, as config.rs `QueueCfg::kinds` reads it): the
 * kinds it names, each once, `ready` last, with any of `ALWAYS_WAITS` it leaves out where the
 * default order has it (`withKind`). The default order without one, from a board or feed that
 * doesn't say. The same order gives the same list, so it can stand in a hook's dependencies.
 */
export function queueKinds(order?: readonly string[] | null): readonly Waits[] {
  // None is the default order and an empty one has only the sessions'; a name with a comma in it
  // isn't two.
  const id = JSON.stringify(order ?? null);
  const kept = KINDS.get(id);
  if (kept) return kept;
  let kinds: Waits[] = [];
  for (const k of order ?? WAITS) {
    const known = WAITS.find((w) => w === k.trim());
    if (known && !kinds.includes(known)) kinds.push(known);
  }
  // The queue comes round to the ones you skipped before it reaches the todos ready to start
  // (`leftAfter`), so they're last in any order.
  kinds = [...kinds.filter((k) => k !== 'ready'), ...kinds.filter((k) => k === 'ready')];
  for (const k of ALWAYS_WAITS) kinds = withKind(kinds, k);
  KINDS.set(id, kinds);
  return kinds;
}

/** The kinds that say something waits on you, which the closed notch counts, in your order: all
 *  but the todos ready to start, of which there are nearly always some, so the count would never
 *  clear. */
export function countedKinds(order: readonly Waits[]): Waits[] {
  return order.filter((w) => w !== 'ready');
}

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
  /** Moved to the top of what waits on you (`Session.raised`), for a row that was. */
  raisedOf: (row: T) => Raised | undefined;
  /** A finished turn that's only yours to read (lanes.ts `waitsFinished`): after the rest of
   *  its kind. */
  finishedOf: (row: T) => boolean;
  /** A review whose repo puts reviews first (`[pull_requests] reviews_first`, `PrItem.reviewFirst`),
   *  for a window that lists reviews. */
  firstOf?: (row: T) => boolean;
  /** Its place in an order of yours: a todo's in your list (barfeed.rs `BarTodo.order`), which a
   *  ready one goes by ahead of how long it's waited, and a due one after it (`byPlace`). */
  orderOf?: (row: T) => number | undefined;
  titleOf: (row: T) => string;
  /** What waits on you in your order (`queueKinds`), which the kinds go in: the default order
   *  without it. A window gets it from its board or feed through `inOrder`. */
  order?: readonly Waits[];
}

// One reads for each window's reads and order, so a window's hooks see them change only when the
// order does.
const ORDERED = new WeakMap<object, Map<readonly Waits[], object>>();

/** `reads` going by `order` (`Reads.order`), the same object each time for the same two. */
export function inOrder<T>(reads: Reads<T>, order: readonly Waits[]): Reads<T> {
  let byOrder = ORDERED.get(reads);
  if (!byOrder) ORDERED.set(reads, (byOrder = new Map()));
  let got = byOrder.get(order) as Reads<T> | undefined;
  if (!got) byOrder.set(order, (got = { ...reads, order }));
  return got;
}

/** Where a row stands in the queue: whether it was moved to the top, what it waits as, whether
 *  it's a review that goes first, whether it's only yours to read, since when, and its key, for
 *  a tie. */
export interface Place {
  key: string;
  waits: Waits;
  /** Where what it waits as comes in the queue: its place in your order (`Reads.order`), or
   *  ahead of them all for a review whose repo puts reviews first. */
  rank: number;
  since: number;
  raised?: Raised;
  first?: boolean;
  finished?: boolean;
  /** Its place in an order of yours (`Reads.orderOf`): ahead of `since` for ready todos, after it
   *  for the rest. */
  order?: number;
}

/** What the queue reads of a row to place it. */
export type PlaceReads<T> = Pick<Reads<T>, 'keyOf' | 'sinceOf' | 'waitsOf' | 'raisedOf' | 'finishedOf' | 'firstOf' | 'orderOf' | 'order'>;

export function placeOf<T>(row: T, reads: PlaceReads<T>): Place {
  const waits = reads.waitsOf(row);
  const first = !!reads.firstOf?.(row);
  return { key: reads.keyOf(row), waits, rank: rankIn(waits, first, reads.order), since: reads.sinceOf(row), raised: reads.raisedOf(row), first, finished: reads.finishedOf(row), order: reads.orderOf?.(row) };
}

/** Where a kind comes in `order` (the default without one): a review whose repo puts reviews
 *  first ahead of them all, and a kind left out of it, which isn't on the window's list, last. */
function rankIn(waits: Waits, first: boolean, order: readonly Waits[] = WAITS): number {
  const at = order.indexOf(waits);
  return first && waits === 'reviews' ? -1 : at < 0 ? WAITS.length : at;
}

/**
 * The order every window works through what waits on you in: the ones moved to the top first,
 * whatever they wait as (lanes.ts `byRaise`); then by kind in your order (`Place.rank`: `WAITS`'
 * unless `[queue] order` says otherwise, with the reviews of a repo that puts them first ahead of
 * them all), each
 * first come, first served, the longest wait first, with the finished turns that are only yours
 * to read after the ones that want something of you. One that comes to wait, or comes back after
 * you answered it, joins the end of its kind and never goes ahead of the ones that have waited
 * longer, unless it's moved to the top. Its wait isn't moved for that: what's been dealt with is
 * kept by it (`Handled`), and the windows say how long it's waited.
 */
export function byPlace(a: Place, b: Place): number {
  // Ready todos go in your order; the rest by how long each has waited, then in your order.
  const yours = (a.order ?? 0) - (b.order ?? 0);
  return byRaise(a.raised, b.raised) || a.rank - b.rank || Number(!!a.finished) - Number(!!b.finished) || (a.waits === 'ready' ? yours : 0) || a.since - b.since || yours || a.key.localeCompare(b.key);
}

/** A window's rows in the queue's order (`byPlace`), whatever order it lists them in. */
export function inQueueOrder<T>(rows: T[], reads: PlaceReads<T>): T[] {
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
 * show it working before the answer's in); one that never waited goes from the top. One moved to
 * the top since the window came to `key` (`came`) goes first, though it's ahead of where you are:
 * you haven't passed it. One moved there before, which you went past, comes round as a skipped
 * one does, so skipping from one moved to the top to another never comes back round to the first.
 */
export function leftAfter<T>(queue: T[], key: string, handled: Handled, reads: PlaceReads<T>, was?: Place | null, came?: number): T[] {
  const rows = inQueueOrder(queue, reads).map((row) => ({ row, at: placeOf(row, reads) }));
  // Where one no longer listed stood goes by the order as it is now: it may have changed since.
  const from = rows.find((r) => r.at.key === key)?.at ?? (was?.key === key ? { ...was, rank: rankIn(was.waits, !!was.first, reads.order) } : null);
  const i = from ? rows.findIndex((r) => byPlace(r.at, from) > 0) : 0;
  const at = i < 0 ? rows.length : i;
  const fresh = (r: { at: Place }) => came != null && r.at.raised != null && r.at.raised.at > came;
  // Those ahead of where you are that are fresh go in among the ones after it, in the queue's
  // order: one moved up by an agent still waits behind one you moved there yourself.
  const before = rows.slice(0, at);
  const ahead = [...before.filter(fresh), ...rows.slice(at)].sort((a, b) => byPlace(a.at, b.at));
  // The todos ready to start come after everything else that waits, however far round: they're
  // there to pick up once nothing does, not ahead of one you skipped past.
  const wrapped = [...ahead, ...before.filter((r) => !fresh(r))];
  const ready = (r: { at: Place }) => r.at.waits === 'ready';
  const order = [...wrapped.filter((r) => !ready(r)), ...wrapped.filter(ready)];
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

/** A todo's row key, where a window's rows hold todos among sessions and PRs (the notch's, the
 *  waiting window's): apart from any session's or PR's. */
export const todoRowKey = (id: string) => `todo:${id}`;

/** Escape puts away a session that can be put away (the notch's). It's each window's own key,
 *  not a step through the queue; the strip's × says it from here. */
export const AWAY_CHORD = 'Esc';

export type Key = Pick<KeyboardEvent, 'key' | 'code' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>;

// Whether a key event is the chord, with exactly its modifiers, as this platform has them.
export { chordPressed };

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
export function stepTo<T>(action: 'next' | 'back', view: Pick<QueueView<T>, 'waiting' | 'reads' | 'was' | 'came'>, key: string, handled: Handled): T | null {
  const left = leftAfter(view.waiting, key, handled, view.reads, view.was, view.came);
  if (action === 'next') return left[0] ?? null;
  // Back is the last of what waits, the ready todos (last of all) only with nothing else.
  return [...left].reverse().find((r) => view.reads.waitsOf(r) !== 'ready') ?? left[left.length - 1] ?? null;
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
  /** It needs you for something else (a call to allow, a question) while its side conversation
   *  answers: ⌘E sets it aside until that's done (board.rs `apply_aside`). */
  asides?: boolean;
  /** It waits on you with nothing to mark seen and nothing ⌘E could answer (a PR waiting on
   *  your review): ⌘E skips it, as it skips one running, rather than staying as it does on a
   *  call to allow. */
  skipsOnSeen?: boolean;
  /** It comes back by itself once it waits anew, as a todo reopened does, whatever it left as:
   *  leaving it isn't kept as dealt with (`left`), since its time can be the same then. */
  comesBack?: boolean;
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
  /** When the window came to the session shown: one moved to the top since then goes next
   *  (`leftAfter`). */
  came?: number;
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
 *  waits, or it's the end of the list): the notch and the waiting window show the end of what
 *  waits (QueueEnd.tsx) when that came of dealing with the one shown; `stay` when it isn't a step at all (a session that left
 *  without having waited, where it stays as it is; ⌘E on a call to allow); `away` when the session
 *  it was done to is no longer the one shown, and the window stays where you put it. */
export type Went = 'on' | 'last' | 'stay' | 'away';

export interface Step<T> {
  went: Went;
  /** The row that takes the window, when it went on. */
  to?: T;
  /** What the strip says on it: how the last one went, or null after a step by hand, which takes
   *  back what it said. Dealing with the last one says it too, for the end of what waits
   *  (QueueEnd.tsx) to say. */
  words?: string | null;
  /** What's been dealt with, after the step. */
  handled: Handled;
}

/** What a key comes to. */
export interface KeyStep<T> extends Step<T> {
  /** The key was the queue's. False when it's the window's own or the card's, or nothing shows
   *  (but for the window's history): the event goes on to whoever's it is. */
  taken: boolean;
  /** The session to mark seen, before the window goes on. */
  markSeen?: string;
  /** The session to set aside until its side conversation answers, before the window goes on. */
  setAside?: string;
  /** Back or forward through the window's history, which says where that is. */
  travel?: 'back' | 'forward';
}

/** The session shown is dealt with: the next that waits takes its place, saying what was done,
 *  or with none, the end of what waits says it. */
function dealt<T>(view: QueueView<T>, handled: Handled, s: Shown, done: string, now: number): Step<T> {
  const after = new Map(handled).set(s.key, s.since ?? now);
  const to = stepTo('next', view, s.key, after);
  const words = wentWords(s.title, done);
  return to ? { went: 'on', to, words, handled: after } : { went: 'last', words, handled: after };
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
 * The session `from` left what waits on you: marked seen, snoozed, set aside, closed or archived.
 * The window goes on as `leftGoesOn` says. A snoozed one isn't kept as dealt with: the snooze
 * shows at once and ends by itself, and when its time comes the session waits on you again, with
 * the status it had. Nor is one set aside, which comes back the same way once its side
 * conversation has answered (board.rs `apply_aside`).
 */
export function left<T>(view: QueueView<T>, handled: Handled, from: string, done: Left, now: number): Step<T> {
  const s = view.shown;
  if (!s || s.key !== from) return { went: 'away', handled };
  if (!leftGoesOn(waits(view.waiting, s.key, view.reads.keyOf), !!view.leftStays)) return { went: 'stay', handled };
  const step = dealt(view, handled, s, `${done}.`, now);
  // What comes back by itself once its time or its session is over isn't kept as dealt with.
  if (done === 'snoozed' || done === 'set aside' || done === 'started' || s.comesBack) step.handled.delete(s.key);
  return step;
}

/** What a key's action comes to on the session shown. */
export function keyStep<T>(view: QueueView<T>, handled: Handled, action: QueueAction, now: number): KeyStep<T> {
  const s = view.shown;
  const no: KeyStep<T> = { taken: false, went: 'stay', handled };
  // The window's history needs no session shown: from the end of what waits, ⌃- goes back to the
  // last one looked at, the one just finished there too.
  if (action === 'historyBack' || action === 'historyForward') return view.travels ? { taken: true, went: 'stay', handled, travel: action === 'historyBack' ? 'back' : 'forward' } : no;
  if (!s) return no;
  /** A step by hand: nothing is dealt with, and the strip takes back what it said. */
  const hand = (to: T | null): KeyStep<T> => (to ? { taken: true, went: 'on', to, words: null, handled } : { taken: true, went: 'last', handled });
  if (action === 'next' || action === 'back') return hand(stepTo(action, view, s.key, handled));
  if (action === 'seen') {
    if (s.asides && view.marksSeen) return { ...left(view, handled, s.key, 'set aside', now), taken: true, setAside: s.key };
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
  return no;
}

/** The first that waits and hasn't been dealt with, for a window that shows nothing: the waiting
 *  window, after the last one, goes to the next that comes to wait on you, and not to one you
 *  just answered that the board still shows waiting. */
export function firstLeft<T>(view: Pick<QueueView<T>, 'waiting' | 'reads'>, handled: Handled): T | undefined {
  return leftAfter(view.waiting, '', handled, view.reads)[0];
}

/** What the strip over the header goes by: what's left after the session shown, the next first,
 *  and whether it's one of those waiting itself. */
export function standing<T>(view: QueueView<T>, handled: Handled): { left: T[]; waiting: boolean } | null {
  const s = view.shown;
  return s ? { left: leftAfter(view.waiting, s.key, handled, view.reads, view.was, view.came), waiting: waits(view.waiting, s.key, view.reads.keyOf) } : null;
}
