import { describe, expect, it } from 'vitest';
import { askedOf, foldQuestions, questionsFolded, showQuestions, shownQuestions } from './questionsBox';

describe('questionsBox', () => {
  it('says what the box shows while it shows it, and whether it’s minimized', () => {
    expect(shownQuestions('s1')).toBeNull();
    const gone = showQuestions('s1', 'ask-1', 'Which color?');
    expect(shownQuestions('s1')).toEqual({ call: 'ask-1', asked: 'Which color?' });
    expect(questionsFolded('s1', 'ask-1')).toBe(false);
    foldQuestions('s1', 'ask-1', true);
    expect(questionsFolded('s1', 'ask-1')).toBe(true);
    foldQuestions('s1', 'ask-1', false);
    expect(questionsFolded('s1', 'ask-1')).toBe(false);
    gone();
    expect(shownQuestions('s1')).toBeNull();
  });

  it('stays minimized as the box goes and comes back for the same call, and not for the next', () => {
    showQuestions('s2', 'ask-1', 'Which color?')();
    foldQuestions('s2', 'ask-1', true);
    showQuestions('s2', 'ask-1', 'Which color?');
    expect(questionsFolded('s2', 'ask-1')).toBe(true);
    showQuestions('s2', 'ask-2', 'Which color?');
    expect(questionsFolded('s2', 'ask-2')).toBe(false);
    expect(questionsFolded('s2', null)).toBe(false);
  });

  it('leaves the next call’s box alone when the one before it goes', () => {
    const first = showQuestions('s3', 'ask-1', 'Which color?');
    showQuestions('s3', 'ask-2', 'Which days?');
    first();
    expect(shownQuestions('s3')?.call).toBe('ask-2');
  });

  it('knows questions by their words, whatever their headers and choices', () => {
    const asked = askedOf([{ question: 'Which color?' }, { question: ' Which days? ' }]);
    expect(asked).toBe('Which color?\nWhich days?');
    expect(askedOf([{ question: 'Which color?' }])).not.toBe(asked);
  });
});
