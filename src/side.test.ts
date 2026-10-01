import { describe, expect, it } from 'vitest';
import { keptLine, turnState } from './side';
import type { SideKept, SideTurn } from './types';

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
