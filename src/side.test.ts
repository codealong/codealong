import { describe, expect, it } from 'vitest';
import { keptLine, keptSummary, STARTING_MS, takesBox, turnState, viewCodec } from './side';
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

describe('keptSummary', () => {
  const kept = (id: string): SideKept => ({ id, key: 'claude:c:1', title: '', startedAt: 1, lastAt: 2, turns: [turn({ done: true })] });
  const open = (id: string): Side => ({ id, key: 'claude:c:1', turns: [turn({ done: true })], running: false, ended: false });

  it('is nothing for a session with none kept', () => {
    expect(keptSummary([], null)).toBeNull();
    // One being answered for the first time isn't kept yet.
    expect(keptSummary([], open('a'))).toBeNull();
  });

  it('counts them, and shows the only one, or the list', () => {
    expect(keptSummary([kept('a')], null)).toEqual({ words: '1 side conversation', show: { kind: 'past', id: 'a' } });
    expect(keptSummary([kept('a'), kept('b')], null)).toEqual({ words: '2 side conversations', show: { kind: 'list' } });
  });

  it('counts the open one with the ones before it, which are what it lists', () => {
    // Only the open one: it's already showing.
    expect(keptSummary([kept('a')], open('a'))).toEqual({ words: '1 side conversation', show: null });
    expect(keptSummary([kept('a'), kept('b')], open('a'))).toEqual({ words: '2 side conversations', show: { kind: 'list' } });
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
    for (const view of [{ kind: 'list' }, { kind: 'past', id: 'a' }, { kind: 'starting', at: 5 }] as const) {
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
});
