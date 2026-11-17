import { describe, expect, it } from 'vitest';
import type { Question } from '../types';
import { answeredBefore, answeredOf, nextQuestion, picksOf } from './QuestionList';

const days: Question = { question: 'Which days?', multiSelect: true, options: [{ label: 'Mon' }, { label: 'Tue' }, { label: 'Wed, late' }] };

describe('picksOf', () => {
  it('marks the choices an answer names, and keeps words of your own', () => {
    expect(picksOf(days, ['Mon', 'Wed, late'])).toEqual({ picked: new Set(['Mon', 'Wed, late']), own: [] });
    expect(picksOf({ ...days, multiSelect: false }, ['Thursday, if it can'])).toEqual({ picked: new Set(), own: ['Thursday, if it can'] });
    expect(picksOf(days, undefined)).toEqual({ picked: new Set(), own: [] });
  });

  it('keeps words of your own whole, commas and all: a reader took apart what was joined', () => {
    expect(picksOf(days, ['Tue', 'before lunch, Mon'])).toEqual({ picked: new Set(['Tue']), own: ['before lunch, Mon'] });
    expect(picksOf(days, ['before lunch, Mon'])).toEqual({ picked: new Set(), own: ['before lunch, Mon'] });
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

describe('answeredBefore', () => {
  const size: Question = { question: 'Which size?', options: [{ label: 'Small' }, { label: 'Large' }] };

  it('picks what you picked, with your own words, from the first question', () => {
    expect(answeredBefore([size, days], { 'Which size?': ['Large'], 'Which days?': ['Mon', 'Wed, late', 'only mornings'] })).toEqual({
      picks: { 'Which size?': ['Large'], 'Which days?': ['Mon', 'Wed, late'] },
      own: { 'Which days?': 'only mornings' },
      at: 0,
    });
    // Words of your own to a question that takes one are all its answer.
    expect(answeredBefore([size], { 'Which size?': ['Medium, please'] })).toEqual({ picks: { 'Which size?': [] }, own: { 'Which size?': 'Medium, please' }, at: 0 });
  });

  it('leaves a question you didn’t answer unpicked, and words of your own whole', () => {
    expect(answeredBefore([size, days], { 'Which days?': ['Mon', 'Tue'] })).toEqual({ picks: { 'Which days?': ['Mon', 'Tue'] }, own: {}, at: 0 });
    // Words of your own stay yours, a choice's name in them or not.
    expect(answeredBefore([days], { 'Which days?': ['before lunch, Mon'] })).toEqual({ picks: { 'Which days?': [] }, own: { 'Which days?': 'before lunch, Mon' }, at: 0 });
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
