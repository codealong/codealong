import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { beingRead, broughtCodec, endSide, keptLine, noteSide, showAnswer, showSides, STARTING_MS, takesBox, turnState, viewCodec } from './side';
import type { Side, SideKept, SideTurn } from './types';

const turn = (t: Partial<SideTurn>): SideTurn => ({ question: 'why?', answer: '', done: false, ...t });

describe('turnState', () => {
  it('says the copy reads the conversation until it has done or said anything', () => {
    expect(turnState(turn({}), true)).toEqual({ kind: 'working', word: 'Reading the conversation' });
  });

  it('keeps the working line, in its own words, through tools and the answer as it comes', () => {
    // A tool that has run, with the copy thinking after it, is still work.
    expect(turnState(turn({ tool: 'Bash · `gcloud compute instances list`' }), true)).toEqual({ kind: 'working' });
    expect(turnState(turn({ tools: ['Bash · `gcloud compute instances list`'] }), true)).toEqual({ kind: 'working' });
    expect(turnState(turn({ answer: 'Because' }), true)).toEqual({ kind: 'working' });
  });

  it('says one asked after the turn the copy is on waits for it', () => {
    expect(turnState(turn({}), false)).toEqual({ kind: 'waits', words: 'Waits for the answer above' });
  });

  it('says nothing once a turn is answered, or ended without one', () => {
    expect(turnState(turn({ answer: 'Because.', done: true }), false)).toBeNull();
    expect(turnState(turn({ answer: 'Because.', done: true }), true)).toBeNull();
    expect(turnState(turn({ done: true, error: 'The copy stopped.' }), true)).toBeNull();
  });
});

describe('keptLine', () => {
  const kept = (turns: SideTurn[]): SideKept => ({ id: 'a', key: 'claude:c:1', title: '', startedAt: 1, lastAt: 2, turns });

  it('is a kept side conversation’s first question, on one line', () => {
    expect(keptLine(kept([turn({ question: '  why a queue?\nand where?', done: true }), turn({ question: 'then?' })]))).toBe('why a queue?');
    expect(keptLine(kept([]))).toBe('');
  });
});

describe('takesBox', () => {
  it('gives the box to the side conversation while anything of it shows', () => {
    expect(takesBox(null, 0)).toBe(false);
    expect(takesBox({ kind: 'list' }, 0)).toBe(true);
    expect(takesBox({ kind: 'past', id: 'a' }, 0)).toBe(true);
  });

  it('holds it for a question whose copy is starting, but not for ever', () => {
    // A window that quit while its copy started never hears it did.
    expect(takesBox({ kind: 'starting', at: 1000 }, 1000 + STARTING_MS - 1)).toBe(true);
    expect(takesBox({ kind: 'starting', at: 1000 }, 1000 + STARTING_MS)).toBe(false);
  });
});

describe('viewCodec', () => {
  it('keeps each view as every window reads it back', () => {
    for (const view of [{ kind: 'list' }, { kind: 'past', id: 'a' }, { kind: 'past', id: 'a', brought: true }, { kind: 'starting', at: 5 }] as const) {
      expect(viewCodec.parse(viewCodec.format(view) ?? '')).toEqual(view);
    }
    // None is no item at all.
    expect(viewCodec.format(null)).toBeNull();
  });

  it('reads what it does not know as none', () => {
    expect(viewCodec.parse('{"kind":"past"}')).toBeUndefined();
    expect(viewCodec.parse('{"kind":"open"}')).toBeUndefined();
    expect(viewCodec.parse('"list"')).toBeUndefined();
  });

  it('reads a view kept before ones could come up by themselves as one you picked', () => {
    expect(viewCodec.parse('{"kind":"past","id":"a"}')).toEqual({ kind: 'past', id: 'a' });
  });
});

describe('showAnswer', () => {
  const key = 'claude:c:answered';
  const shows = (at: number, open = false) => {
    showSides(key, null);
    showAnswer(key, { id: 'a', open, at });
    return beingRead(key);
  };

  it('shows the one that answered once, put away or not', () => {
    expect(shows(10)).toBe('a');
    expect(shows(10)).toBeNull();
    // Another answer comes up again.
    expect(shows(20)).toBe('a');
  });

  it('leaves an open one above the box, where it shows by itself', () => {
    expect(shows(30, true)).toBeNull();
  });

  describe('across windows', () => {
    // Local storage, which every window shares and keeps across restarts.
    let items: Map<string, string>;
    beforeEach(() => {
      items = new Map();
      vi.stubGlobal('window', { localStorage: { getItem: (k: string) => items.get(k) ?? null, setItem: (k: string, v: string) => void items.set(k, v), removeItem: (k: string) => void items.delete(k) } });
    });
    afterEach(() => vi.unstubAllGlobals());
    /** A window just opened, or the app restarted: side.ts as it loads, with nothing brought up yet. */
    const opened = async () => {
      vi.resetModules();
      return import('./side');
    };
    const shownIn = (side: typeof import('./side'), at: number, open = false) => {
      side.showSides(key, null);
      side.showAnswer(key, { id: 'a', open, at });
      return side.beingRead(key);
    };

    it('keeps one put away from coming back in another window, or once the app restarts', async () => {
      // A session waiting on you for a question it asked can't be seen, so it keeps the answer, and
      // each window that came to it brought the closed one up again (2026-11-03).
      expect(shownIn(await opened(), 40)).toBe('a');
      expect(shownIn(await opened(), 40)).toBeNull();
      // A later answer comes up, once.
      expect(shownIn(await opened(), 50)).toBe('a');
      expect(shownIn(await opened(), 50)).toBeNull();
      // The way it's written never changes, or one put away comes back with an update.
      expect(items.get(`codealong.sideanswer.${key}`)).toBe('50');
    });

    it('brings up an open one again once it has closed, as the server’s restart closes it', async () => {
      // Open, it shows above the box by itself, and nothing's kept of it.
      expect(shownIn(await opened(), 60, true)).toBeNull();
      expect(items.has(`codealong.sideanswer.${key}`)).toBe(false);
      // The app restarted with the server: the same answer, closed, shows to read.
      expect(shownIn(await opened(), 60)).toBe('a');
      // The server alone restarted, under a window that brought it up open.
      const here = await opened();
      expect(shownIn(here, 70, true)).toBeNull();
      expect(shownIn(here, 70)).toBe('a');
      expect(shownIn(here, 70)).toBeNull();
      expect(shownIn(await opened(), 70)).toBeNull();
    });
  });
});

describe('broughtCodec', () => {
  it('reads back when an answer came, and nothing else', () => {
    expect(broughtCodec.parse(broughtCodec.format(1793741766496) ?? '')).toBe(1793741766496);
    for (const raw of ['', 'x', '-5', '1.5', '{"at":5}']) expect(broughtCodec.parse(raw)).toBeUndefined();
  });
});

describe('noteSide', () => {
  const side = (questions: number): Side => ({ id: 's1', key: 'claude:c:reopened', turns: Array.from({ length: questions }, (_, i) => turn({ question: `q${i}`, done: true })), running: false, ended: false });
  /** Whether the window holds an open side conversation for the session: Close only asks the
   *  engine to close one it holds. */
  const holdsOpen = () => {
    const close = vi.fn(() => Promise.resolve());
    endSide('claude:c:reopened', close);
    return close.mock.calls.length > 0;
  };

  it('drops word of a closed one that came late, and takes it once it is asked again', () => {
    noteSide(side(1));
    expect(holdsOpen()).toBe(true);
    // Closed just now: what its copy said before that comes late.
    noteSide(side(1));
    expect(holdsOpen()).toBe(false);
    // Asked again as it was read, it's open with a question more (side.rs `Reading`).
    noteSide(side(2));
    expect(holdsOpen()).toBe(true);
  });
});
