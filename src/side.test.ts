import { describe, expect, it } from 'vitest';
import { keptLine, keptSummary, turnState } from './side';
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
