import { describe, expect, it } from 'vitest';
import { foldQuestions, questionsBox, showQuestions, unfoldQuestions } from './questionsBox';

describe('questionsBox', () => {
  it('says what the box does with the questions while it shows them', () => {
    expect(questionsBox('s1')).toBeNull();
    const gone = showQuestions('s1', 'ask-1');
    expect(questionsBox('s1')).toBe('open');
    foldQuestions('s1', 'ask-1', true);
    expect(questionsBox('s1')).toBe('folded');
    gone();
    expect(questionsBox('s1')).toBeNull();
  });

  it('stays minimized as the box goes and comes back for the same call, and not for the next', () => {
    showQuestions('s2', 'ask-1');
    foldQuestions('s2', 'ask-1', true);
    showQuestions('s2', 'ask-1')();
    showQuestions('s2', 'ask-1');
    expect(questionsBox('s2')).toBe('folded');
    showQuestions('s2', 'ask-2');
    expect(questionsBox('s2')).toBe('open');
  });

  it('shows a minimized box again from the conversation', () => {
    showQuestions('s3', 'ask-1');
    foldQuestions('s3', 'ask-1', true);
    unfoldQuestions('s3');
    expect(questionsBox('s3')).toBe('open');
    // With no box showing questions, there's nothing to show.
    unfoldQuestions('s4');
    expect(questionsBox('s4')).toBeNull();
  });

  it('leaves the next call’s box alone when the one before it goes', () => {
    const first = showQuestions('s5', 'ask-1');
    showQuestions('s5', 'ask-2');
    first();
    expect(questionsBox('s5')).toBe('open');
  });
});
