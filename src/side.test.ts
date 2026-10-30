import { describe, expect, it, vi } from 'vitest';
import { beingRead, endSide, keptLine, noteSide, showAnswer, showSides, STARTING_MS, takesBox, turnState, viewCodec } from './side';
import type { Side, SideKept, SideTurn } from './types';

const turn = (t: Partial<SideTurn>): SideTurn => ({ question: 'why?', answer: '', done: false, ...t });

describe('turnState', () => {
  it('says what a turn without its answer is waiting on', () => {
    expect(turnState(turn({}), true)).toBe('Reading the conversation…');
    expect(turnState(turn({}), false)).toBe('Next');
  });

  it('leaves it to the answer and the tool once there is one', () => {
    expect(turnState(turn({ answer: 'Because' }), true)).toBeNull();
    expect(turnState(turn({ tool: 'Bash · `git status`' }), true)).toBeNull();
    expect(turnState(turn({ answer: 'Because.', done: true }), false)).toBeNull();
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

  it('shows the one that answered once in a window, put away or not', () => {
    expect(shows(10)).toBe('a');
    expect(shows(10)).toBeNull();
    // Another answer comes up again.
    expect(shows(20)).toBe('a');
  });

  it('leaves an open one above the box, where it shows by itself', () => {
    expect(shows(30, true)).toBeNull();
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
