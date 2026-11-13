import { describe, expect, it } from 'vitest';
import type { Question } from './types';
import { askedOf, calledOf, foldQuestions, questionsFolded, showCall, shownCall } from './questionsBox';

describe('questionsBox', () => {
  it('says what the box shows while it shows it, and whether it’s minimized', () => {
    expect(shownCall('s1')).toBeNull();
    const gone = showCall('s1', 'ask-1', 'Which color?');
    expect(shownCall('s1')).toEqual({ call: 'ask-1', asked: 'Which color?' });
    expect(questionsFolded('s1', 'ask-1')).toBe(false);
    foldQuestions('s1', 'ask-1', true);
    expect(questionsFolded('s1', 'ask-1')).toBe(true);
    foldQuestions('s1', 'ask-1', false);
    expect(questionsFolded('s1', 'ask-1')).toBe(false);
    gone();
    expect(shownCall('s1')).toBeNull();
  });

  it('stays minimized as the box goes and comes back for the same call, and not for the next', () => {
    showCall('s2', 'ask-1', 'Which color?')();
    foldQuestions('s2', 'ask-1', true);
    showCall('s2', 'ask-1', 'Which color?');
    expect(questionsFolded('s2', 'ask-1')).toBe(true);
    showCall('s2', 'ask-2', 'Which color?');
    expect(questionsFolded('s2', 'ask-2')).toBe(false);
    expect(questionsFolded('s2', null)).toBe(false);
  });

  it('leaves the next call’s box alone when the one before it goes', () => {
    const first = showCall('s3', 'ask-1', 'Which color?');
    showCall('s3', 'ask-2', 'Which days?');
    first();
    expect(shownCall('s3')?.call).toBe('ask-2');
  });

  it('knows questions by their words and their choices, whatever their headers', () => {
    const approach: Question = { question: 'Which approach?', options: [{ label: 'Keep the cache' }, { label: 'Remove the cache' }] };
    const asked = askedOf([approach]);
    const headed: Question = { ...approach, question: ' Which approach? ', header: 'Approach', multiSelect: true };
    expect(askedOf([headed])).toBe(asked);
    expect(askedOf([{ ...approach, options: [{ label: 'Use polling' }, { label: 'Use events' }] }])).not.toBe(asked);
    expect(askedOf([approach, { question: 'Which days?', options: [] }])).not.toBe(asked);
  });

  it('knows a call to allow by its label, and never for questions', () => {
    const called = calledOf('Bash · `npm test`');
    expect(calledOf(' Bash · `npm test` ')).toBe(called);
    expect(calledOf('Bash · `npm run build`')).not.toBe(called);
    // A call labeled as questions are asked still isn't them.
    const color: Question = { question: 'Which color?', options: [{ label: 'Red' }] };
    expect(calledOf('Asks you · Which color?')).not.toBe(askedOf([color]));
  });
});
