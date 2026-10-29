import { describe, expect, it } from 'vitest';
import type { Raised } from './types';
import { answered, chordPressed, firstLeft, inQueueOrder, keyStep, left, leftAfter, leftGoesOn, placeOf, QUEUE_CHORDS, queueKey, seenStep, standing, statusWaits, stepAlong, stepTo, waits, type Handled, type QueueView, type Reads, type Shown, type Waits } from './queue';

// A window's rows, as the bar's pills are: a session's key, when its status began, what it waits
// on you as, its title.
interface Row {
  key: string;
  since: number;
  /** Left out for one that runs, which the queue never lists. */
  waits?: Waits;
  /** A finished turn, or a reply that asks you something. */
  seenable?: boolean;
  raised?: Raised;
}
const READS: Reads<Row> = {
  keyOf: (r) => r.key,
  sinceOf: (r) => r.since,
  waitsOf: (r) => r.waits ?? 'unread',
  raisedOf: (r) => r.raised,
  titleOf: (r) => `Queue ${r.key}`,
};
const NOW = 1_000;
const keys = (rows: Row[]) => rows.map((r) => r.key);

// What the preview checks walk: A and B need you (B on a call to allow, which ⌘E doesn't answer),
// C and D are finished turns, and W runs.
const A: Row = { key: 'A', since: 10, waits: 'needs', seenable: true };
const B: Row = { key: 'B', since: 20, waits: 'needs' };
const C: Row = { key: 'C', since: 30, waits: 'unread', seenable: true };
const D: Row = { key: 'D', since: 40, waits: 'unread', seenable: true };
const W: Row = { key: 'W', since: 5 };
const WAITING = [A, B, C, D];
const shown = (r: Row): Shown => ({ key: r.key, title: READS.titleOf(r), since: r.since, seenable: !!r.seenable });
/** Sessions' panel in the main window: ⌘E is its own, a row that left without waiting stays, and
 *  ⌥↑ ⌥↓ and ⌃- ⌃⇧- are the window's. */
const sessions = (on: Row | null, waiting = WAITING): QueueView<Row> => ({ waiting, reads: READS, shown: on && shown(on), marksSeen: true, leftStays: true });
/** The session in the notch, and the waiting window: a card that marks seen itself, over
 *  a list it steps along, with a history of its own. */
const card = (on: Row | null, waiting = WAITING, along = [...WAITING, W]): QueueView<Row> => ({ waiting, reads: READS, shown: on && shown(on), along, travels: true });
const none: Handled = new Map();

const key = (init: Partial<KeyboardEvent>) => ({ key: '', code: '', metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...init });

describe('the queue’s keys', () => {
  it('knows each action by its chords', () => {
    expect(queueKey(key({ key: ']', code: 'BracketRight', metaKey: true }))).toBe('next');
    expect(queueKey(key({ key: 'ArrowDown', altKey: true, shiftKey: true }))).toBe('next');
    expect(queueKey(key({ key: '[', code: 'BracketLeft', metaKey: true }))).toBe('back');
    expect(queueKey(key({ key: 'ArrowUp', altKey: true, shiftKey: true }))).toBe('back');
    expect(queueKey(key({ key: 'e', metaKey: true }))).toBe('seen');
    expect(queueKey(key({ key: 'E', code: 'KeyE', metaKey: true }))).toBe('seen');
    expect(queueKey(key({ key: 'ArrowDown', altKey: true }))).toBe('down');
    expect(queueKey(key({ key: 'ArrowUp', altKey: true }))).toBe('up');
    expect(queueKey(key({ key: '-', code: 'Minus', ctrlKey: true }))).toBe('historyBack');
    expect(queueKey(key({ key: '_', code: 'Minus', ctrlKey: true, shiftKey: true }))).toBe('historyForward');
  });

  it('takes ] and [ by where the keys are, with ⌥ held or not, and - by what it types or where it is', () => {
    // A layout that types ] or [ with ⌥ has another character on the key.
    expect(queueKey(key({ key: '+', code: 'BracketRight', metaKey: true }))).toBe('next');
    expect(queueKey(key({ key: '‘', code: 'BracketRight', metaKey: true, altKey: true }))).toBe('next');
    expect(queueKey(key({ key: ']', code: 'Digit6', metaKey: true, altKey: true }))).toBeNull();
    expect(queueKey(key({ key: 'ü', code: 'BracketLeft', metaKey: true }))).toBe('back');
    expect(queueKey(key({ key: '“', code: 'BracketLeft', metaKey: true, altKey: true }))).toBe('back');
    expect(queueKey(key({ key: '[', code: 'Digit5', metaKey: true, altKey: true }))).toBeNull();
    // ⌘⇧[ is a different chord.
    expect(queueKey(key({ key: '{', code: 'BracketLeft', metaKey: true, shiftKey: true }))).toBeNull();
    expect(queueKey(key({ key: 'ß', code: 'Minus', ctrlKey: true }))).toBe('historyBack');
    expect(queueKey(key({ key: '-', code: 'Slash', ctrlKey: true }))).toBe('historyBack');
  });

  it('leaves other chords alone: a key with more or fewer modifiers isn’t the queue’s', () => {
    // The arrows alone are the list's or the text's; ⌃⌘↑ ⌃⌘↓ move a pin.
    expect(queueKey(key({ key: 'ArrowDown' }))).toBeNull();
    expect(queueKey(key({ key: 'ArrowDown', shiftKey: true }))).toBeNull();
    expect(queueKey(key({ key: 'ArrowDown', ctrlKey: true, metaKey: true }))).toBeNull();
    expect(queueKey(key({ key: 'ArrowDown', altKey: true, metaKey: true }))).toBeNull();
    // ⇧⌘] is the next tab's in other apps, ⌃⌘E the emoji picker's, ⌥⌘E nothing's here.
    expect(queueKey(key({ key: ']', code: 'BracketRight', metaKey: true, shiftKey: true }))).toBeNull();
    expect(queueKey(key({ key: 'e', metaKey: true, ctrlKey: true }))).toBeNull();
    expect(queueKey(key({ key: 'e', metaKey: true, altKey: true }))).toBeNull();
    expect(queueKey(key({ key: 'e', metaKey: true, shiftKey: true }))).toBeNull();
    expect(queueKey(key({ key: 'e' }))).toBeNull();
    // ⌘- zooms, and ⌃⌥- isn't the history's.
    expect(queueKey(key({ key: '-', code: 'Minus', metaKey: true }))).toBeNull();
    expect(queueKey(key({ key: '-', code: 'Minus', ctrlKey: true, altKey: true }))).toBeNull();
  });

  it('has every chord its table lists, and no chord twice', () => {
    const all = Object.values(QUEUE_CHORDS).flat();
    expect(new Set(all).size).toBe(all.length);
    expect(chordPressed('Cmd+E', key({ key: 'e', metaKey: true }))).toBe(true);
    expect(chordPressed('Cmd+E', key({ key: 'e', metaKey: true, altKey: true }))).toBe(false);
    // ` by what it types, or where it is where it's a dead key.
    expect(chordPressed('Ctrl+`', key({ key: '`', code: 'Backquote', ctrlKey: true }))).toBe(true);
    expect(chordPressed('Ctrl+`', key({ key: 'Dead', code: 'Backquote', ctrlKey: true }))).toBe(true);
    expect(chordPressed('Ctrl+`', key({ key: '`', code: 'Backquote', ctrlKey: true, shiftKey: true }))).toBe(false);
  });
});

describe('the queue’s order', () => {
  it('is the most urgent kind first, each first come, first served, whatever order a window lists them in', () => {
    const review: Row = { key: 'R', since: 5, waits: 'reviews' };
    expect(keys(inQueueOrder([D, review, C, B, A], READS))).toEqual(['A', 'B', 'R', 'C', 'D']);
    // A key settles a tie.
    const twin: Row = { ...C, key: 'C2' };
    expect(keys(inQueueOrder([twin, C], READS))).toEqual(['C', 'C2']);
  });

  it('puts one that waits again after you answered it behind the ones that waited longer', () => {
    // C's turn ended again after you answered it: D has waited longer now.
    const again = WAITING.map((r) => (r.key === 'C' ? { ...r, since: 50 } : r));
    expect(keys(inQueueOrder(again, READS))).toEqual(['A', 'B', 'D', 'C']);
    expect(keys(leftAfter(again, 'B', new Map([['C', 30]]), READS))).toEqual(['D', 'C', 'A']);
  });

  it('goes to the ones moved to the top first, whatever they wait as, yours before an agent’s, the latest first', () => {
    const top = (r: Row, at: number, agent = false): Row => ({ ...r, raised: { at, agent } });
    // D, a finished turn you moved up, goes ahead of A and B, which need you.
    expect(keys(inQueueOrder([A, B, C, top(D, 100)], READS))).toEqual(['D', 'A', 'B', 'C']);
    // The latest you moved first, and an agent's after every one of yours.
    expect(keys(inQueueOrder([top(A, 100), top(B, 300, true), C, top(D, 200)], READS))).toEqual(['D', 'A', 'B', 'C']);
    // Dealt with, it's on to the rest as they were, from the top.
    const raised = [A, B, C, top(D, 100)];
    expect(keys(leftAfter(raised, 'D', none, READS))).toEqual(['A', 'B', 'C']);
    // Its wait stays its own, so what you've dealt with stays dealt with.
    expect(placeOf(top(D, 100), READS).since).toBe(D.since);
    expect(keys(leftAfter(raised, 'A', new Map([['D', D.since]]), READS))).toEqual(['B', 'C']);
  });

  it('goes next to one moved to the top while you were on another, but not back to one you went past', () => {
    const top = (r: Row, at: number): Row => ({ ...r, raised: { at } });
    // On B since 100, D was moved up at 150: next is D, then on from B.
    expect(keys(leftAfter([A, B, C, top(D, 150)], 'B', none, READS, null, 100))).toEqual(['D', 'C', 'A']);
    // Moved up before you came to B (you went past it), it comes round as a skipped one does.
    expect(keys(leftAfter([A, B, C, top(D, 50)], 'B', none, READS, null, 100))).toEqual(['C', 'D', 'A']);
    // Skipping from one moved up to the other, and on, never goes back round to the first.
    const both = [top(A, 60), top(B, 50), C, D];
    expect(keys(leftAfter(both, 'A', none, READS, null, 70))).toEqual(['B', 'C', 'D']);
    expect(keys(leftAfter(both, 'B', none, READS, null, 80))).toEqual(['C', 'D', 'A']);
    expect(keys(leftAfter(both, 'C', none, READS, null, 90))).toEqual(['D', 'A', 'B']);
    // And a window that shows the one moved up says it's next.
    expect(stepTo('next', { ...card(B, [A, B, C, top(D, 150)]), came: 100 }, 'B', none)?.key).toBe('D');
  });

  it('names what a session waits as by its status', () => {
    expect(statusWaits('needs')).toBe('needs');
    expect(statusWaits('error')).toBe('needs');
    expect(statusWaits('unread')).toBe('unread');
  });
});

describe('what’s left after a session', () => {
  it('goes on from the session answered, then back to the ones skipped', () => {
    expect(keys(leftAfter(WAITING, 'A', none, READS))).toEqual(['B', 'C', 'D']);
    expect(keys(leftAfter(WAITING, 'C', none, READS))).toEqual(['D', 'A', 'B']);
    // Not in the queue, and never was while shown (it runs): from the top.
    expect(keys(leftAfter(WAITING, 'W', none, READS))).toEqual(['A', 'B', 'C', 'D']);
    expect(keys(leftAfter(WAITING, 'gone', none, READS))).toEqual(['A', 'B', 'C', 'D']);
  });

  it('goes on from where an answered one stood once the list no longer has it', () => {
    // B's call was allowed and the board shows it working before the answer's in: on to C, and
    // A, which you skipped to get to B, still comes after the rest.
    const rest = [A, C, D];
    expect(keys(leftAfter(rest, 'B', none, READS, placeOf(B, READS)))).toEqual(['C', 'D', 'A']);
    expect(answered({ ...card(B, rest), was: placeOf(B, READS) }, none, 'B', 'Allowed it.', NOW)).toMatchObject({ went: 'on', to: C });
    expect(standing({ ...card(B, rest), was: placeOf(B, READS) }, none)?.left.map((r) => r.key)).toEqual(['C', 'D', 'A']);
    // Where another session stood says nothing of this one's.
    expect(keys(leftAfter(rest, 'W', none, READS, placeOf(B, READS)))).toEqual(['A', 'C', 'D']);
  });

  it('takes one that comes to wait ahead of where you are round after the rest, as one you skipped', () => {
    // On C, your turn: N comes to need you. It's done after D, before going round to A and B.
    const n: Row = { key: 'N', since: 50, waits: 'needs' };
    expect(keys(leftAfter([...WAITING, n], 'C', none, READS))).toEqual(['D', 'A', 'B', 'N']);
  });

  it('leaves out what you dealt with until it waits on you anew', () => {
    const handled: Handled = new Map([['B', 20]]);
    expect(keys(leftAfter(WAITING, 'A', handled, READS))).toEqual(['C', 'D']);
    const again = WAITING.map((r) => (r.key === 'B' ? { ...r, since: 50 } : r));
    expect(keys(leftAfter(again, 'A', handled, READS))).toEqual(['B', 'C', 'D']);
  });

  it('says whether a session waits itself, dealt with or not', () => {
    expect(waits(WAITING, 'B', READS.keyOf)).toBe(true);
    expect(waits(WAITING, 'W', READS.keyOf)).toBe(false);
  });

  it('gives the strip what’s left and whether the session shown waits', () => {
    expect(standing(sessions(null), none)).toBeNull();
    const on = standing(sessions(A), none);
    expect(on && keys(on.left)).toEqual(['B', 'C', 'D']);
    expect(on?.waiting).toBe(true);
    // On one that doesn't wait, the strip says what does, not what more.
    const running = standing(card(W), new Map([['A', 10]]));
    expect(running && keys(running.left)).toEqual(['B', 'C', 'D']);
    expect(running?.waiting).toBe(false);
  });

  it('gives a window that shows nothing the first not dealt with, and the next that comes to wait', () => {
    expect(firstLeft(card(null), none)).toBe(A);
    // After the last: what the board still shows waiting until it catches up isn't gone back to.
    const all: Handled = new Map(WAITING.map((r) => [r.key, r.since]));
    expect(firstLeft(card(null), all)).toBeUndefined();
    // One that waits on you anew, or for the first time, is.
    const fresh: Row = { key: 'E', since: 60, waits: 'unread' };
    expect(firstLeft(card(null, [...WAITING, fresh]), all)).toBe(fresh);
    expect(firstLeft(card(null, WAITING.map((r) => (r.key === 'C' ? { ...r, since: 70 } : r))), all)?.key).toBe('C');
    expect(firstLeft(card(null, []), none)).toBeUndefined();
  });
});

describe('⌘] ⌥⇧↓ and ⌘[ ⌥⇧↑', () => {
  it('skip to the next that waits, and back to the one before', () => {
    const q = { waiting: WAITING, reads: READS };
    expect(stepTo('next', q, 'A', none)?.key).toBe('B');
    expect(stepTo('back', q, 'B', none)?.key).toBe('A');
    // From the first, back is round to the last; from the last, on is round to the first.
    expect(stepTo('back', q, 'A', none)?.key).toBe('D');
    expect(stepTo('next', q, 'D', none)?.key).toBe('A');
    // From one that runs, the first that waits.
    expect(stepTo('next', q, 'W', none)?.key).toBe('A');
  });

  it('deal with nothing, and take back what the strip said', () => {
    const step = keyStep(sessions(C), none, 'next', NOW);
    expect(step).toMatchObject({ taken: true, went: 'on', to: D, words: null });
    expect(step.handled).toBe(none);
    expect(keyStep(card(B), none, 'back', NOW)).toMatchObject({ taken: true, went: 'on', to: A, words: null });
  });

  it('pass over the ones dealt with: a skipped one comes round again, they don’t', () => {
    // A and B answered, C skipped: from D the next is C again.
    const handled: Handled = new Map([['A', 10], ['B', 20]]);
    expect(keyStep(sessions(D), handled, 'next', NOW).to).toBe(C);
    expect(keyStep(sessions(C), handled, 'back', NOW).to).toBe(D);
  });

  it('go nowhere with nothing else waiting, and are still the queue’s keys', () => {
    expect(keyStep(sessions(A, [A]), none, 'next', NOW)).toMatchObject({ taken: true, went: 'last' });
    expect(keyStep(sessions(A, [A]), none, 'next', NOW).to).toBeUndefined();
    expect(keyStep(card(W, []), none, 'back', NOW)).toMatchObject({ taken: true, went: 'last' });
  });

  it('are nobody’s with no session shown', () => {
    expect(keyStep(sessions(null), none, 'next', NOW)).toMatchObject({ taken: false });
    expect(keyStep(card(null), none, 'historyBack', NOW)).toMatchObject({ taken: false });
  });
});

describe('⌘E', () => {
  it('is written once: seen, skip or stay', () => {
    // A finished turn or a reply that asks: marked seen, whether it waits on you or is snoozed.
    expect(seenStep(true, true)).toBe('seen');
    expect(seenStep(true, false)).toBe('seen');
    // Nothing to mark seen: one that runs is skipped as ⌘] skips; a call to allow stays.
    expect(seenStep(false, false)).toBe('skip');
    expect(seenStep(false, true)).toBe('stay');
  });

  it('marks a finished turn seen and goes on, where the window runs it', () => {
    const step = keyStep(sessions(A), none, 'seen', NOW);
    expect(step).toMatchObject({ taken: true, markSeen: 'A', went: 'on', to: B, words: 'Queue A: marked seen.' });
    expect([...step.handled]).toEqual([['A', 10]]);
  });

  it('skips a row that waits with nothing to mark seen or answer, as a review asked of you', () => {
    const step = keyStep({ ...card(B), shown: { ...shown(B), seenable: false, skipsOnSeen: true } }, none, 'seen', NOW);
    expect(step).toMatchObject({ taken: true, went: 'on', to: C, words: null });
    expect(step.markSeen).toBeUndefined();
    expect([...step.handled]).toEqual([]);
  });

  it('marks seen and stays on a row that didn’t wait on you', () => {
    // A finished turn that's snoozed, say: it isn't in the queue.
    const snoozed: Row = { key: 'S', since: 1, waits: 'unread', seenable: true };
    const step = keyStep(sessions(snoozed), none, 'seen', NOW);
    expect(step).toMatchObject({ taken: true, markSeen: 'S', went: 'stay' });
    expect(step.to).toBeUndefined();
  });

  it('leaves one to mark seen to the card that shows it', () => {
    expect(keyStep(card(A), none, 'seen', NOW)).toMatchObject({ taken: false });
    expect(keyStep(card(A), none, 'seen', NOW).markSeen).toBeUndefined();
  });

  it('does nothing on a call to allow', () => {
    expect(keyStep(sessions(B), none, 'seen', NOW)).toMatchObject({ taken: false, went: 'stay' });
    expect(keyStep(card(B), none, 'seen', NOW)).toMatchObject({ taken: false, went: 'stay' });
  });

  it('goes on from one that runs as ⌘] does, and marks nothing seen, in every window', () => {
    for (const view of [sessions(W), card(W)]) {
      const step = keyStep(view, none, 'seen', NOW);
      expect(step).toMatchObject({ taken: true, went: 'on', to: A, words: null });
      expect(step.markSeen).toBeUndefined();
      expect(step.handled).toBe(none);
    }
    // One that waited when it was shown and runs now is no longer in the list.
    expect(keyStep(card(B, [A, C, D]), none, 'seen', NOW)).toMatchObject({ taken: true, to: A });
  });
});

describe('⌥↑ ⌥↓', () => {
  const along = [A, B, C, W];

  it('step along everything listed, the running ones too, stopping at the ends', () => {
    expect(stepAlong(along, 'B', 1, READS.keyOf)?.key).toBe('C');
    expect(stepAlong(along, 'B', -1, READS.keyOf)?.key).toBe('A');
    expect(stepAlong(along, 'A', -1, READS.keyOf)?.key).toBe('A');
    expect(stepAlong(along, 'W', 1, READS.keyOf)?.key).toBe('W');
    // From one the rows don't list: the first, or the last going back.
    expect(stepAlong(along, 'gone', 1, READS.keyOf)?.key).toBe('A');
    expect(stepAlong(along, 'gone', -1, READS.keyOf)?.key).toBe('W');
    expect(stepAlong([], 'A', 1, READS.keyOf)).toBeNull();
  });

  it('show the one stepped to, and take the key even at an end', () => {
    expect(keyStep(card(C, WAITING, along), none, 'down', NOW)).toMatchObject({ taken: true, went: 'on', to: W, words: null });
    expect(keyStep(card(W, WAITING, along), none, 'down', NOW)).toMatchObject({ taken: true, went: 'last' });
    expect(keyStep(card(A, WAITING, along), none, 'up', NOW).to).toBeUndefined();
  });

  it('are the window’s own where it gives no list to step along', () => {
    expect(keyStep(sessions(B), none, 'down', NOW)).toMatchObject({ taken: false });
  });
});

describe('⌃- ⌃⇧-', () => {
  it('go through the window’s history where it has one for them', () => {
    expect(keyStep(card(A), none, 'historyBack', NOW)).toMatchObject({ taken: true, travel: 'back' });
    expect(keyStep(card(A), none, 'historyForward', NOW)).toMatchObject({ taken: true, travel: 'forward' });
  });

  it('are the window’s own elsewhere', () => {
    expect(keyStep(sessions(A), none, 'historyBack', NOW)).toMatchObject({ taken: false });
    expect(keyStep(sessions(A), none, 'historyBack', NOW).travel).toBeUndefined();
  });
});

describe('⌘⏎: answered', () => {
  it('goes on to the next that waits, saying how it went, and keeps the one answered out', () => {
    const first = answered(sessions(A), none, 'A', 'sent.', NOW);
    expect(first).toMatchObject({ went: 'on', to: B, words: 'Queue A: sent.' });
    expect([...first.handled]).toEqual([['A', 10]]);
    // What was dealt with isn't changed in place: a render may still be reading it.
    expect(none.size).toBe(0);
    const second = answered(sessions(B), first.handled, 'B', 'Allowed it.', NOW);
    expect(second).toMatchObject({ went: 'on', to: C, words: 'Queue B: Allowed it.' });
    // ⌘] from C skips to D; from D the skipped one comes round again, the ones dealt with don't.
    expect(keyStep(sessions(C), second.handled, 'next', NOW).to).toBe(D);
    expect(keyStep(sessions(D), second.handled, 'next', NOW).to).toBe(C);
  });

  it('says so when nothing else waits, and it’s still dealt with', () => {
    const step = answered(card(D), new Map([['A', 10], ['B', 20], ['C', 30]]), 'D', 'sent.', NOW);
    expect(step.went).toBe('last');
    expect(step.to).toBeUndefined();
    expect(step.handled.get('D')).toBe(40);
    // The end of what waits says how it went.
    expect(step.words).toBe('Queue D: sent.');
  });

  it('leaves the window where you put it when the answer comes in after you moved on', () => {
    const step = answered(sessions(C), none, 'A', 'Allowed it.', NOW);
    expect(step).toMatchObject({ went: 'away' });
    expect(step.to).toBeUndefined();
    // And the session as it was: it comes round again unless the board shows it answered.
    expect(step.handled).toBe(none);
    expect(answered(sessions(null), none, 'A', 'sent.', NOW).went).toBe('away');
  });

  it('goes on from a session that doesn’t wait itself to the first that does', () => {
    expect(answered(card(W), none, 'W', 'sent.', NOW)).toMatchObject({ went: 'on', to: A, words: 'Queue W: sent.' });
  });

  it('takes one the list no longer has as dealt with now', () => {
    const gone: Shown = { key: 'G', title: 'The last one', seenable: false };
    const step = answered({ ...card(null), shown: gone }, none, 'G', 'sent.', NOW);
    expect(step).toMatchObject({ went: 'on', to: A, words: 'The last one: sent.' });
    expect(step.handled.get('G')).toBe(NOW);
  });
});

describe('a session that left what waits on you', () => {
  it('takes the window on from one that waited, saying what was done', () => {
    expect(left(sessions(B), none, 'B', 'archived', NOW)).toMatchObject({ went: 'on', to: C, words: 'Queue B: archived.' });
    expect(left(card(C), none, 'C', 'closed', NOW)).toMatchObject({ went: 'on', to: D, words: 'Queue C: closed.' });
    expect(left(card(A), none, 'A', 'marked seen', NOW)).toMatchObject({ went: 'on', to: B, words: 'Queue A: marked seen.' });
  });

  it('stays on a row that didn’t wait, and goes on where the session was all the window showed', () => {
    expect(leftGoesOn(true, true)).toBe(true);
    expect(leftGoesOn(false, true)).toBe(false);
    expect(leftGoesOn(false, false)).toBe(true);
    // Archiving down Sessions' old sessions is no step through the queue.
    const stays = left(sessions(W), none, 'W', 'archived', NOW);
    expect(stays).toMatchObject({ went: 'stay' });
    expect(stays.handled).toBe(none);
    // The notch and the quick panel have nothing left to show of it.
    expect(left(card(W), none, 'W', 'archived', NOW)).toMatchObject({ went: 'on', to: A, words: 'Queue W: archived.' });
  });

  it('says so when nothing else waits, and how the last one went, for the end of what waits', () => {
    expect(left(card(A, [A]), none, 'A', 'marked seen', NOW)).toMatchObject({ went: 'last', words: 'Queue A: marked seen.' });
    expect(left(card(W, []), none, 'W', 'archived', NOW)).toMatchObject({ went: 'last', words: 'Queue W: archived.' });
  });

  it('doesn’t keep a snoozed one as dealt with: it waits again when its time comes', () => {
    const step = left(sessions(A), none, 'A', 'snoozed', NOW);
    expect(step).toMatchObject({ went: 'on', to: B, words: 'Queue A: snoozed.' });
    expect(step.handled.has('A')).toBe(false);
    expect(left(card(A, [A]), none, 'A', 'snoozed', NOW).handled.has('A')).toBe(false);
    expect(left(sessions(A), none, 'A', 'archived', NOW).handled.has('A')).toBe(true);
  });

  it('moves nothing when it isn’t the one shown', () => {
    expect(left(card(B), none, 'A', 'archived', NOW)).toMatchObject({ went: 'away' });
  });
});
