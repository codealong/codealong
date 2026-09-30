import { describe, expect, it } from 'vitest';
import type { Question } from '../types';
import { picksOf } from './QuestionList';

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
