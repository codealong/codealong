import { describe, expect, it } from 'vitest';
import type { Question } from '../types';
import { answeredOf, nextQuestion, picksOf } from './QuestionList';

const days: Question = { question: 'Which days?', multiSelect: true, options: [{ label: 'Mon' }, { label: 'Tue' }, { label: 'Wed, late' }] };

describe('picksOf', () => {
  it('marks the choices an answer names, and keeps words of your own', () => {
    expect(picksOf(days, ['Mon', 'Wed, late'])).toEqual({ picked: new Set(['Mon', 'Wed, late']), own: [] });
    expect(picksOf({ ...days, multiSelect: false }, ['Thursday, if it can'])).toEqual({ picked: new Set(), own: ['Thursday, if it can'] });
    expect(picksOf(days, undefined)).toEqual({ picked: new Set(), own: [] });
  });

  it('reads several picks older versions joined with commas', () => {
    expect(picksOf(days, ['Mon, Tue, only mornings'])).toEqual({ picked: new Set(['Mon', 'Tue']), own: ['only mornings'] });
    // A question that takes one isn't split: its answer is one choice or your own words.
    expect(picksOf({ ...days, multiSelect: false }, ['Mon, Tue'])).toEqual({ picked: new Set(), own: ['Mon, Tue'] });
  });
});

describe('answeredOf', () => {
  const limit: Question = { question: 'Which limit?', header: 'Limit', options: [{ label: '100 days' }, { label: '45 days' }] };

  it('lists your answers in the order asked, by header, several picks together', () => {
    const answers = { 'Which days?': ['Mon', 'Tue'], 'Which limit?': ['100 days'] };
    expect(answeredOf([limit, days], answers)).toEqual([
      { question: 'Which limit?', header: 'Limit', answer: '100 days' },
      { question: 'Which days?', header: undefined, answer: 'Mon, Tue' },
    ]);
  });

  it('leaves out a question you didn’t answer, and keeps an answer to one the call isn’t read for', () => {
    expect(answeredOf([limit, days], { 'Which days?': ['Wed, late'] })).toEqual([{ question: 'Which days?', header: undefined, answer: 'Wed, late' }]);
    expect(answeredOf([], { 'Which limit?': ['Why a limit at all?'] })).toEqual([{ question: 'Which limit?', answer: 'Why a limit at all?' }]);
  });
});

describe('nextQuestion', () => {
  it('goes on to the next question without an answer, then back to the first', () => {
    expect(nextQuestion([true, false, false], 0)).toBe(1);
    expect(nextQuestion([false, true, false], 0)).toBe(2);
    expect(nextQuestion([false, false, true], 2)).toBe(0);
  });

  it('has nowhere to go once every other question has an answer', () => {
    expect(nextQuestion([false, true, true], 0)).toBeNull();
    expect(nextQuestion([true, true, true], 1)).toBeNull();
    expect(nextQuestion([false], 0)).toBeNull();
  });
});
