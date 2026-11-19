// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest';
import { act, engine, mountHook, press, settle } from '../testing/dom';
import { DWELL_MS } from '../history';
import * as fx from '../testing/fixtures';
import type { BarItem, BarReview } from '../types';
import { useNotchStage } from './NotchSession';

// The notch's keyboard while it shows a session under the bar's pills (`useNotchStage`): what
// puts it away, the queue's keys over the bar's own list, and typing going to the box. The
// card it shows isn't mounted: the stage is the bar's part, and its keys are heard on the page.

const pill = (id: string, group: BarItem['group'], since: number): BarItem => fx.barItem({ key: `claude:northwind:${id}`, title: `${id[0].toUpperCase()}${id.slice(1)}`, group, since });
const keyOf = (id: string) => `claude:northwind:${id}`;

// Atlas needs you and Cedar's turn ended; Wren runs. A PR waits on your review between them.
const ITEMS = [pill('wren', 'working', 5), pill('cedar', 'unread', 30), pill('atlas', 'needs', 10)];
const REVIEW: BarReview = fx.barReview({ key: 'northwind/app#12', number: 12, title: 'Kestrel login', repo: 'app', repoSlug: 'northwind/app', url: 'https://github.com/northwind/app/pull/12', project: 'Northwind', since: 20 });
const NO_TODOS = { due: [], ready: [] };

/** What the bar has on it. */
interface OnBar {
  items: BarItem[];
  reviews: BarReview[];
  docked: boolean;
}

/** The bar docked in the notch, with these pills and that review on it. The page is the bar's
 *  window, as the engine knows it (bar.rs `LABEL`): what it sends the bar reaches this page. */
function notch(over: Partial<OnBar> = {}) {
  engine.window('bar');
  const bar: OnBar = { items: ITEMS, reviews: [REVIEW], docked: true, ...over };
  const stage = mountHook((on: OnBar) => useNotchStage(on.items, on.reviews, NO_TODOS, (key) => on.items.some((i) => i.key === key) || on.reviews.some((r) => r.key === key), on.docked), bar);
  return {
    stage,
    /** What shows: a session's or review's key while it's open. */
    get shown() {
      return stage.current.open ? stage.current.key : null;
    },
    show: (id: string) => act(() => stage.current.show(keyOf(id))),
    /** The bar's feed changed. */
    now: (change: Partial<OnBar>) => stage.rerender({ ...bar, ...change }),
  };
}

/** The box of the card the notch shows, on the page as SessionCard draws it. */
function box(): HTMLTextAreaElement {
  const card = document.body.appendChild(document.createElement('div'));
  card.className = 'ca-notch-session';
  const composer = card.appendChild(document.createElement('div'));
  composer.className = 'ca-composer';
  return composer.appendChild(document.createElement('textarea'));
}

describe('the notch, while a session shows', () => {
  it('takes the keyboard as it opens on one, and hands it back as it’s put away', () => {
    const n = notch();
    expect(n.shown).toBeNull();
    n.show('atlas');
    expect([n.shown, n.stage.current.keys]).toEqual([keyOf('atlas'), true]);
    act(() => n.stage.current.close());
    expect([n.shown, n.stage.current.keys]).toEqual([null, false]);
    expect(engine.sent('notch_keys')).toEqual([{ on: true }, { on: false }]);
  });

  it('is put away by Escape, and by ⌘W, ⌘Q and ⌘H, which mean the app in front while its menu bar shows', () => {
    for (const chord of ['Esc', 'Cmd+W', 'Cmd+Q', 'Cmd+H']) {
      const n = notch();
      n.show('atlas');
      expect(press(chord).defaultPrevented, chord).toBe(true);
      expect(n.shown, chord).toBeNull();
      n.stage.unmount();
    }
  });

  // The peek closes on ⇧⌘Q, ⇧⌘H and ⇧⌘W too (Peek.tsx takes ⌘ with or without ⇧), and the notch
  // doesn't: its `mod` admits ⇧⌘ and the `!e.shiftKey` beside it takes that back. They read as
  // one rule in the comments of both, so one of the two is wrong; which is the owner's to say.
  // The quick panel is a third spelling: ⌘Q and ⌘H with or without ⇧, and no ⌘W. The test after
  // this one holds what the notch does today, for each of the three keys.
  it.todo('closes on ⇧⌘W, ⇧⌘Q and ⇧⌘H as the peek does, or the peek stops closing on them');

  it('leaves ⌘W, ⌘Q and ⌘H with ⇧, ⌃ or ⌥ held, and every other key, to the page', () => {
    const n = notch();
    n.show('atlas');
    const held = ['W', 'Q', 'H'].flatMap((key) => [`Shift+Cmd+${key}`, `Ctrl+Cmd+${key}`, `Alt+Cmd+${key}`]);
    for (const chord of [...held, 'Cmd+K', 'w', 'q', 'h', 'Enter']) {
      expect(press(chord).defaultPrevented, chord).toBe(false);
      expect(n.shown, chord).toBe(keyOf('atlas'));
    }
  });

  it('hears nothing while it’s closed', () => {
    const n = notch();
    for (const chord of ['Esc', 'Cmd+W', 'Cmd+]']) expect(press(chord).defaultPrevented, chord).toBe(false);
    expect(n.shown).toBeNull();
    expect(engine.sent('notch_keys')).toEqual([]);
  });

  it('goes through what waits with the queue’s keys, a review among the sessions, and along the running ones with ⌥↓', () => {
    const n = notch();
    n.show('atlas');
    expect(press('Cmd+]').defaultPrevented).toBe(true);
    expect([n.shown, n.stage.current.kind]).toEqual(['northwind/app#12', 'review']);
    press('Cmd+]');
    expect([n.shown, n.stage.current.kind]).toEqual([keyOf('cedar'), 'session']);
    // Round to the first again, and back from it to the last.
    press('Alt+Shift+Down');
    expect(n.shown).toBe(keyOf('atlas'));
    press('Cmd+[');
    expect(n.shown).toBe(keyOf('cedar'));
    // Along the bar's list, the running one is after those that wait.
    press('Alt+Down');
    expect(n.shown).toBe(keyOf('wren'));
    expect(press('Alt+Down').defaultPrevented).toBe(true);
    expect(n.shown).toBe(keyOf('wren'));
  });

  it('goes back with ⌃- to the one looked at before, not one only passed on the way, and forward again with ⌃⇧-', () => {
    vi.useFakeTimers();
    try {
      const n = notch();
      /** Long enough on the one shown for it to count as looked at (history.ts). */
      const look = () => act(() => void vi.advanceTimersByTime(DWELL_MS));
      n.show('atlas');
      look();
      // Past the review without stopping, to Cedar.
      press('Cmd+]');
      press('Cmd+]');
      look();
      expect(press('Ctrl+-').defaultPrevented).toBe(true);
      expect(n.shown).toBe(keyOf('atlas'));
      press('Ctrl+Shift+-');
      expect(n.shown).toBe(keyOf('cedar'));
    } finally {
      vi.useRealTimers();
    }
  });

  it('leaves ⌘E on a finished turn to its card, which marks it seen, and skips a review with it', () => {
    const n = notch();
    n.show('cedar');
    expect(press('Cmd+E').defaultPrevented).toBe(false);
    expect(n.shown).toBe(keyOf('cedar'));
    press('Cmd+[');
    expect(n.shown).toBe('northwind/app#12');
    expect(press('Cmd+E').defaultPrevented).toBe(true);
    expect(n.shown).toBe(keyOf('cedar'));
  });

  it('gives the queue’s keys to a dialog over the card, but still closes on Escape', () => {
    const n = notch();
    n.show('atlas');
    const dialog = document.body.appendChild(document.createElement('div'));
    dialog.setAttribute('aria-modal', 'true');
    expect(press('Cmd+]').defaultPrevented).toBe(false);
    expect(n.shown).toBe(keyOf('atlas'));
    press('Esc');
    expect(n.shown).toBeNull();
  });

  it('puts what’s typed with nothing focused into the box', () => {
    const n = notch();
    n.show('atlas');
    const field = box();
    // A chord isn't typing, nor is a key that types nothing.
    for (const chord of ['Cmd+V', 'Ctrl+A', 'Down', 'Tab']) press(chord);
    expect(document.activeElement).toBe(document.body);
    press('h');
    expect(document.activeElement).toBe(field);
  });

  it('leaves typing where the keyboard already is', () => {
    const n = notch();
    n.show('atlas');
    box();
    const other = document.body.appendChild(document.createElement('input'));
    other.focus();
    press('h');
    expect(document.activeElement).toBe(other);
  });

  /** The notch on Atlas with no review asked, then both sessions dealt with: the end of what waits. */
  const atTheEnd = () => {
    const n = notch({ reviews: [] });
    n.show('atlas');
    act(() => n.stage.current.left(keyOf('atlas'), 'archived'));
    expect(n.shown).toBe(keyOf('cedar'));
    act(() => n.stage.current.left(keyOf('cedar'), 'marked seen'));
    return n;
  };

  it('shows the end of what waits when the last one is dealt with, which keeps the keyboard until Escape puts it away', () => {
    const n = atTheEnd();
    expect([n.stage.current.open, n.stage.current.kind, n.stage.current.keys, n.stage.current.ended]).toEqual([true, 'end', true, 'Cedar: marked seen.']);
    expect(press('Esc').defaultPrevented).toBe(true);
    expect(n.stage.current.open).toBe(false);
  });

  it('goes on from the end by itself to what comes to wait next, and not back to one just dealt with', () => {
    const n = atTheEnd();
    // The bar still shows the two dealt with as waiting for a moment: the end stays.
    n.now({ reviews: [], items: [...ITEMS] });
    expect(n.stage.current.kind).toBe('end');
    // Dune's turn ends: it takes the end's place, with nothing pressed.
    n.now({ reviews: [], items: [...ITEMS, pill('dune', 'unread', 50)] });
    expect([n.shown, n.stage.current.kind, n.stage.current.ended]).toEqual([keyOf('dune'), 'session', null]);
  });

  it('stays closed when something comes to wait after the end was put away', () => {
    const n = atTheEnd();
    press('Esc');
    n.now({ reviews: [], items: [...ITEMS, pill('dune', 'unread', 50)] });
    expect(n.shown).toBeNull();
  });

  it('loses the keyboard when the engine tells the bar another app took it, and only then', async () => {
    const n = notch();
    await settle();
    n.show('atlas');
    // What's sent to another window isn't the bar's to hear.
    act(() => engine.emitTo('main', 'notch-keys', false));
    expect([n.shown, n.stage.current.keys]).toEqual([keyOf('atlas'), true]);
    act(() => engine.emitTo('bar', 'notch-keys', false));
    expect([n.shown, n.stage.current.keys]).toEqual([keyOf('atlas'), false]);
    // A click in the card takes it back.
    act(() => n.stage.current.takeKeys());
    expect(n.stage.current.keys).toBe(true);
    expect(engine.sent('notch_keys')).toEqual([{ on: true }, { on: true }]);
  });

  it('is put away when the bar leaves the notch', () => {
    const n = notch();
    n.show('atlas');
    n.now({ docked: false });
    expect(n.shown).toBeNull();
    expect(engine.sent('notch_keys')).toEqual([{ on: true }, { on: false }]);
  });
});
