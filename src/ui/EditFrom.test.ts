import { describe, expect, it } from 'vitest';
import type { Question } from '../types';
import { answersPrompt, rewindTitle, rewindWords } from './EditFrom';

const size: Question = { question: 'Which size?', header: 'Size', options: [{ label: 'Small' }, { label: 'Large' }] };
const days: Question = { question: 'Which days?', multiSelect: true, options: [{ label: 'Mon' }, { label: 'Tue' }] };

describe('answersPrompt', () => {
  it('says which questions the answers are to, a line each, as Claude Code hands them over', () => {
    expect(answersPrompt([size, days], { 'Which size?': 'Large', 'Which days?': ['Mon', 'only mornings'] })).toBe(
      'My answers to the questions you just asked:\n"Which size?"="Large"\n"Which days?"="Mon, only mornings"',
    );
    expect(answersPrompt([size], { 'Which size?': 'Medium, please' })).toBe('My answers to the question you just asked:\n"Which size?"="Medium, please"');
  });

  it('leaves out a question you didn’t answer', () => {
    expect(answersPrompt([size, days], { 'Which days?': ['Tue'] })).toBe('My answers to the questions you just asked:\n"Which days?"="Tue"');
  });
});

describe('rewind words', () => {
  it('say one question or several', () => {
    expect(rewindTitle(false)).toBe('Rewind to this question');
    expect(rewindTitle(true)).toBe('Rewind to these questions');
    expect(rewindWords({ title: 'Kestrel lead' }, false)).toMatch(/^A copy of “Kestrel lead” goes back to this question .* only the conversation goes back\.$/);
  });
});
