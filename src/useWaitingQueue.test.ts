// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, engine, mountHook, press, settle } from './testing/dom';
import { useEffect, useState } from 'react';
import type { Left } from './menus';
import { setPlatform } from './platform';
import { useSideAnswering } from './side';
import * as fx from './testing/fixtures';
import type { Session, Side, Status } from './types';
import { SESSION_READS, shownSession, useWaitingQueue } from './useWaitingQueue';

// The rules a step goes by are queue.ts's, with tests of their own (queue.test.ts). These hold
// the hook that runs them for a window: a key pressed in a window that shows one session at a
// time, and what the window shows and says after it.

interface Row {
  session: Session;
}
const row = (id: string, status: Status, since: number, over: Partial<Session> = {}): Row => ({ session: fx.session({ key: `claude:northwind:${id}`, title: `${id[0].toUpperCase()}${id.slice(1)}`, status, statusSince: since, ...over }) });
const keyOf = (id: string) => `claude:northwind:${id}`;
const WAITS: Status[] = ['needs', 'error', 'unread'];

// Atlas asks you something and Birch waits on a call to allow (which ⌘E doesn't answer); Cedar's
// and Dune's turns ended; Wren runs. They've waited in that order.
const atlas = row('atlas', 'needs', 10, { asks: true });
const birch = row('birch', 'needs', 20);
const cedar = row('cedar', 'unread', 30);
const dune = row('dune', 'unread', 40);
const wren = row('wren', 'working', 5);
// Listed as a window might list them, which isn't the order they're worked through in.
const LISTED = [wren, dune, birch, atlas, cedar];

// side.ts keeps what the engine tells it of side conversations for as long as the page lives, so
// a test that has one gives it to a session of its own, waiting on a call to allow as Birch does,
// and what was told of it is taken back as the test ends.
const told: Side[] = [];
/** The engine says the session's side conversation was asked something its copy is answering. */
function answering(id: string): Side {
  const side: Side = { id: `side-${id}`, key: keyOf(id), turns: [{ question: 'Which migration does this call run?', answer: '', done: false }], running: true, ended: false };
  told.push(side);
  act(() => engine.emit('side', side));
  return side;
}
afterEach(() => {
  for (const side of told.splice(0)) act(() => engine.emit('side', { ...side, turns: [], running: false, ended: true }));
});

interface Opens {
  rows?: Row[];
  on: string | null;
  /** Sessions' panel in the main window: ⌘E is the window's to run, a row that left without
   *  waiting stays, and ⌥↑ ⌥↓ and ⌃- ⌃⇧- are its own. Else a card, as the notch and the waiting
   *  window show one: ⌥↑ ⌥↓ step along its list, and it has a history. */
  main?: boolean;
}

/** A window that shows one session at a time and gives its keys to the queue after its own, as
 *  the waiting window and the notch do (Waiting.tsx, NotchSession.tsx). */
function open(opens: Opens) {
  const shows: string[] = [];
  const seen: string[] = [];
  const travels: string[] = [];
  const hook = mountHook(() => {
    const [rows, setRows] = useState(opens.rows ?? LISTED);
    const [on, setOn] = useState(opens.on && keyOf(opens.on));
    const at = rows.find((r) => r.session.key === on);
    // A card hears whether the session's side conversation answers (SessionCard.tsx).
    useSideAnswering(on ?? undefined);
    const queue = useWaitingQueue<Row>({
      waiting: rows.filter((r) => WAITS.includes(r.session.status)),
      along: opens.main ? undefined : rows,
      reads: SESSION_READS,
      // One the list no longer has still shows until you leave it.
      shown: on ? (at ? shownSession(at.session) : { key: on, title: 'The last one', seenable: false }) : null,
      show: (to) => {
        shows.push(to.session.title);
        setOn(to.session.key);
      },
      markSeen: opens.main ? (key) => seen.push(key) : undefined,
      travel: opens.main
        ? undefined
        : (dir) => {
            travels.push(dir);
            return true;
          },
      leftStays: opens.main,
    });
    const { onKey } = queue;
    useEffect(() => {
      const hear = (e: KeyboardEvent) => {
        if (onKey(e)) e.preventDefault();
      };
      window.addEventListener('keydown', hear);
      return () => window.removeEventListener('keydown', hear);
    }, [onKey]);
    return { on, rows, queue, setRows, setOn };
  });
  const title = () => [...hook.current.rows, ...LISTED].find((r) => r.session.key === hook.current.on)?.session.title ?? hook.current.on;
  return {
    hook,
    shows,
    seen,
    travels,
    /** The session showing, by its title. */
    get on() {
      return title();
    },
    get queue() {
      return hook.current.queue;
    },
    /** The board changed: these are the window's rows now. */
    rows: (rows: Row[]) => act(() => hook.current.setRows(rows)),
    /** The window dealt with the one shown; with nothing else waiting it shows nothing, as the
     *  windows show the end of what waits. */
    went: (step: () => string) =>
      act(() => {
        if (step() === 'last') hook.current.setOn(null);
      }),
  };
}

describe('a window’s way through what waits on you', () => {
  it('goes on with ⌘] to the one that has waited longest after this one, and back with ⌘[, whatever order the window lists them in', () => {
    const w = open({ on: 'atlas' });
    expect(press('Cmd+]').defaultPrevented).toBe(true);
    expect(w.on).toBe('Birch');
    press('Cmd+]');
    expect(w.on).toBe('Cedar');
    expect(press('Cmd+[').defaultPrevented).toBe(true);
    expect(w.on).toBe('Birch');
    // ⌥⇧↓ and ⌥⇧↑ are the same keys.
    press('Alt+Shift+Down');
    press('Alt+Shift+Down');
    expect(w.on).toBe('Dune');
    press('Alt+Shift+Up');
    expect(w.on).toBe('Cedar');
    expect(w.shows).toEqual(['Birch', 'Cedar', 'Birch', 'Cedar', 'Dune', 'Cedar']);
  });

  it('comes round from the last to the ones skipped, and back from the first to the last', () => {
    const w = open({ on: 'dune' });
    press('Cmd+]');
    expect(w.on).toBe('Atlas');
    press('Cmd+[');
    expect(w.on).toBe('Dune');
  });

  it('says what is left after the one shown, and which is next', () => {
    const w = open({ on: 'birch' });
    expect(w.queue.queue).toMatchObject({ next: { title: 'Cedar' }, left: 3, ready: 0, waiting: true });
    // The strip's Next is ⌘].
    act(() => w.queue.queue?.onNext());
    expect(w.on).toBe('Cedar');
  });

  it('goes on from one that runs to everything that waits, the first first', () => {
    const w = open({ on: 'wren' });
    expect(w.queue.queue).toMatchObject({ next: { title: 'Atlas' }, left: 4, waiting: false });
    press('Cmd+]');
    expect(w.on).toBe('Atlas');
  });

  it('takes ⌘] and ⌘[ and stays where it is with nothing else waiting', () => {
    const w = open({ rows: [cedar, wren], on: 'cedar' });
    expect(press('Cmd+]').defaultPrevented).toBe(true);
    expect(press('Cmd+[').defaultPrevented).toBe(true);
    expect(w.on).toBe('Cedar');
    expect(w.shows).toEqual([]);
  });

  it('leaves every key that isn’t the queue’s to the window', () => {
    const w = open({ on: 'atlas' });
    for (const chord of ['j', 'Down', 'Enter', 'Cmd+K', 'Shift+Cmd+]', 'Ctrl+Cmd+E', 'Shift+Cmd+E']) expect(press(chord).defaultPrevented, chord).toBe(false);
    expect(w.on).toBe('Atlas');
  });

  it('has nothing to step from with no session shown, so the keys are the window’s', () => {
    const w = open({ on: null });
    expect(press('Cmd+]').defaultPrevented).toBe(false);
    expect(w.shows).toEqual([]);
    // What a window that shows nothing goes to: the first that waits.
    expect(w.queue.first?.session.title).toBe('Atlas');
    expect(w.queue.pending.map((r) => r.session.title)).toEqual(['Atlas', 'Birch', 'Cedar', 'Dune']);
  });

  it('presses the same keys on Linux, where ⌘ is Ctrl', () => {
    setPlatform('linux');
    const w = open({ on: 'atlas' });
    expect(press('Cmd+]').ctrlKey).toBe(true);
    expect(w.on).toBe('Birch');
    press('Alt+Shift+Down');
    expect(w.on).toBe('Cedar');
    press('Cmd+[');
    expect(w.on).toBe('Birch');
  });
});

describe('⌥↓ and ⌥↑ in a window that shows a card', () => {
  it('step along everything it lists, the running ones too, and stop at the ends', () => {
    const w = open({ on: 'dune' });
    press('Alt+Up');
    expect(w.on).toBe('Wren');
    // The top of the list: the key is still the queue's, and goes nowhere.
    expect(press('Alt+Up').defaultPrevented).toBe(true);
    expect(w.on).toBe('Wren');
    LISTED.forEach(() => press('Alt+Down'));
    expect(w.on).toBe('Cedar');
  });

  it('are the main window’s own, whose arrows move along its list in every view', () => {
    const w = open({ on: 'dune', main: true });
    expect(press('Alt+Down').defaultPrevented).toBe(false);
    expect(press('Alt+Up').defaultPrevented).toBe(false);
    expect(w.on).toBe('Dune');
  });
});

describe('⌃- and ⌃⇧-', () => {
  it('go back and forward through the window’s history where it has one', () => {
    const w = open({ on: 'atlas' });
    expect(press('Ctrl+-').defaultPrevented).toBe(true);
    expect(press('Ctrl+Shift+-').defaultPrevented).toBe(true);
    expect(w.travels).toEqual(['back', 'forward']);
    // Where it went is the window's to show: the queue shows nothing itself.
    expect(w.shows).toEqual([]);
  });

  it('go back from the end of what waits too, with no session shown', () => {
    const w = open({ on: null });
    expect(press('Ctrl+-').defaultPrevented).toBe(true);
    expect(w.travels).toEqual(['back']);
  });

  it('are the main window’s own', () => {
    open({ on: 'atlas', main: true });
    expect(press('Ctrl+-').defaultPrevented).toBe(false);
  });
});

describe('⌘E', () => {
  it('marks a finished turn seen and goes on, where the window runs it', () => {
    const w = open({ on: 'cedar', main: true });
    expect(press('Cmd+E').defaultPrevented).toBe(true);
    expect(w.seen).toEqual([keyOf('cedar')]);
    expect(w.on).toBe('Dune');
    expect(w.queue.notice).toEqual({ text: 'Cedar: marked seen.', error: false });
  });

  it('is the card’s where a card shows the session: the queue doesn’t take it', () => {
    const w = open({ on: 'cedar' });
    expect(press('Cmd+E').defaultPrevented).toBe(false);
    expect(w.on).toBe('Cedar');
  });

  it('does nothing on a call to allow, which it doesn’t answer', () => {
    const w = open({ on: 'birch', main: true });
    expect(press('Cmd+E').defaultPrevented).toBe(false);
    expect(w.on).toBe('Birch');
    expect(w.seen).toEqual([]);
  });

  it('goes on from one that runs as ⌘] does, and marks nothing seen', () => {
    const w = open({ on: 'wren', main: true });
    expect(press('Cmd+E').defaultPrevented).toBe(true);
    expect(w.on).toBe('Atlas');
    expect(w.seen).toEqual([]);
  });

  it('sets one that needs you aside while its side conversation answers, and goes on', async () => {
    const w = open({ rows: [...LISTED, row('elm', 'needs', 25)], on: 'elm', main: true });
    await settle();
    answering('elm');
    expect(press('Cmd+E').defaultPrevented).toBe(true);
    expect(engine.sent('aside_sessions')).toEqual([{ keys: [keyOf('elm')] }]);
    expect(w.on).toBe('Cedar');
    expect(w.queue.notice).toEqual({ text: 'Elm: set aside.', error: false });
    // It comes back by itself once the side conversation has answered: it isn't kept as dealt with.
    press('Cmd+[');
    expect(w.on).toBe('Elm');
  });

  it('leaves it to you again once its side conversation has answered', async () => {
    const w = open({ rows: [...LISTED, row('gorse', 'needs', 25)], on: 'gorse', main: true });
    await settle();
    const side = answering('gorse');
    act(() => engine.emit('side', { ...side, turns: [{ ...side.turns[0], answer: 'The one that adds the index.', done: true }], running: false }));
    expect(press('Cmd+E').defaultPrevented).toBe(false);
    expect(engine.sent('aside_sessions')).toEqual([]);
    expect(w.on).toBe('Gorse');
  });

  it('says why on the session shown when the engine doesn’t set it aside', async () => {
    engine.answer('aside_sessions', () => {
      throw new Error('That session is on another device, which isn’t connected.');
    });
    const w = open({ rows: [...LISTED, row('fern', 'needs', 25)], on: 'fern', main: true });
    await settle();
    answering('fern');
    press('Cmd+E');
    await settle();
    expect(w.on).toBe('Cedar');
    expect(w.queue.notice).toEqual({ text: 'That session is on another device, which isn’t connected.', error: true });
  });
});

describe('a session dealt with', () => {
  const sent = (w: ReturnType<typeof open>, id: string, done = 'Sent.') => w.went(() => w.queue.answered(keyOf(id), done));
  const left = (w: ReturnType<typeof open>, id: string, done: Left) => w.went(() => w.queue.left(keyOf(id), done));

  it('gives its place to the next that waits, which says how it went until you step again', () => {
    const w = open({ on: 'atlas' });
    sent(w, 'atlas');
    expect(w.on).toBe('Birch');
    expect(w.queue.notice).toEqual({ text: 'Atlas: Sent.', error: false });
    press('Cmd+]');
    expect(w.on).toBe('Cedar');
    expect(w.queue.notice).toBeNull();
  });

  it('says how it went on the one gone on to, not on whichever a click shows instead', () => {
    const w = open({ on: 'atlas' });
    sent(w, 'atlas');
    act(() => w.hook.current.setOn(keyOf('dune')));
    expect(w.queue.notice).toBeNull();
    act(() => w.hook.current.setOn(keyOf('birch')));
    expect(w.queue.notice).toEqual({ text: 'Atlas: Sent.', error: false });
  });

  it('stops saying how the last one went after a while, but not why one failed', () => {
    vi.useFakeTimers();
    try {
      const w = open({ on: 'atlas' });
      sent(w, 'atlas');
      expect(w.queue.notice?.text).toBe('Atlas: Sent.');
      act(() => void vi.advanceTimersByTime(60_000));
      expect(w.queue.notice).toBeNull();
      act(() => void w.queue.say({ text: 'Couldn’t send to Birch: its terminal is gone.', error: true }));
      act(() => void vi.advanceTimersByTime(60_000));
      expect(w.queue.notice).toEqual({ text: 'Couldn’t send to Birch: its terminal is gone.', error: true });
    } finally {
      vi.useRealTimers();
    }
  });

  it('isn’t come back to while the board still shows it waiting, until it waits anew', () => {
    const w = open({ on: 'atlas' });
    sent(w, 'atlas');
    sent(w, 'birch');
    expect(w.on).toBe('Cedar');
    // Back from Cedar passes the two dealt with: Dune is the only other one left.
    press('Cmd+[');
    expect(w.on).toBe('Dune');
    expect(w.queue.queue).toMatchObject({ next: { title: 'Cedar' }, left: 1 });
    // Atlas asks again: it has waited since after the answer, and comes round after the rest.
    w.rows(LISTED.map((r) => (r === atlas ? row('atlas', 'needs', 50, { asks: true }) : r)));
    expect(w.queue.queue).toMatchObject({ next: { title: 'Atlas' }, left: 2 });
    press('Cmd+]');
    expect(w.on).toBe('Atlas');
  });

  it('comes round again when its message didn’t go after all', () => {
    const w = open({ on: 'atlas' });
    sent(w, 'atlas');
    act(() => w.queue.again(keyOf('atlas')));
    press('Cmd+[');
    expect(w.on).toBe('Atlas');
  });

  it('leaves the window where you put it when the answer comes in after you moved on', () => {
    const w = open({ on: 'atlas' });
    press('Cmd+]');
    sent(w, 'atlas');
    expect(w.on).toBe('Birch');
    expect(w.queue.notice).toBeNull();
    // And Atlas wasn't dealt with: it's still among what's left.
    expect(w.queue.queue).toMatchObject({ left: 3 });
  });

  it('takes the window on when it left what waits, saying what was done', () => {
    const w = open({ on: 'cedar' });
    left(w, 'cedar', 'archived');
    expect(w.on).toBe('Dune');
    expect(w.queue.notice).toEqual({ text: 'Cedar: archived.', error: false });
    // Snoozed, it comes back by itself when its time comes, so it isn't kept as dealt with.
    left(w, 'dune', 'snoozed');
    expect(w.on).toBe('Atlas');
    press('Cmd+[');
    expect(w.on).toBe('Dune');
  });

  it('stays, as a row in the main window’s list, when it left without having waited', () => {
    const w = open({ on: 'wren', main: true });
    left(w, 'wren', 'archived');
    expect(w.on).toBe('Wren');
    expect(w.shows).toEqual([]);
  });

  it('goes on from a card all the same, which has nothing else of it to show', () => {
    const w = open({ on: 'wren' });
    left(w, 'wren', 'archived');
    expect(w.on).toBe('Atlas');
  });

  it('ends what waits when it was the last: nothing shows, the end says how it went, and the next to come is first', () => {
    const w = open({ rows: [cedar, wren], on: 'cedar' });
    left(w, 'cedar', 'marked seen');
    expect(w.on).toBeNull();
    expect(w.queue.queue).toBeUndefined();
    expect(w.queue.ended).toBe('Cedar: marked seen.');
    // The board still shows Cedar waiting for a moment: it isn't what comes next.
    expect(w.queue.first).toBeNull();
    w.rows([cedar, wren, dune]);
    expect(w.queue.first?.session.title).toBe('Dune');
    // Something shows again: the end has nothing more to say.
    act(() => w.hook.current.setOn(keyOf('dune')));
    expect(w.queue.ended).toBeNull();
  });

  it('starts afresh when the window opens anew: nothing dealt with, nothing said', () => {
    const w = open({ on: 'atlas' });
    sent(w, 'atlas');
    act(() => w.queue.fresh());
    expect(w.queue.notice).toBeNull();
    press('Cmd+[');
    expect(w.on).toBe('Atlas');
  });
});

describe('a session that goes away under the window', () => {
  it('is gone on from where it stood, not from the top, once the board shows it working', () => {
    const w = open({ on: 'birch' });
    // Birch's call was allowed: the board shows it working before the window hears of the answer.
    w.rows(LISTED.map((r) => (r === birch ? row('birch', 'working', 60) : r)));
    expect(w.queue.queue).toMatchObject({ next: { title: 'Cedar' }, left: 3, waiting: false });
    press('Cmd+]');
    expect(w.on).toBe('Cedar');
  });

  it('is gone on from where it stood when the board no longer has it at all', () => {
    const w = open({ on: 'birch' });
    w.rows(LISTED.filter((r) => r !== birch));
    press('Cmd+]');
    expect(w.on).toBe('Cedar');
    // Back from there is Atlas: Birch is no longer among them.
    press('Cmd+[');
    expect(w.on).toBe('Atlas');
  });

  it('is answered from where it stood too, and what comes next says how it went', () => {
    const w = open({ on: 'birch' });
    w.rows(LISTED.filter((r) => r !== birch));
    w.went(() => w.queue.answered(keyOf('birch'), 'Allowed.'));
    expect(w.on).toBe('Cedar');
    expect(w.queue.notice).toEqual({ text: 'The last one: Allowed.', error: false });
  });

  it('leaves the others in their order when one the window isn’t on goes', () => {
    const w = open({ on: 'atlas' });
    w.rows(LISTED.filter((r) => r !== birch));
    press('Cmd+]');
    expect(w.on).toBe('Cedar');
  });
});
