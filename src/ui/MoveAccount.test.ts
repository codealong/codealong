import { describe, expect, it } from 'vitest';
import { moveWords } from './MoveAccount';

describe('moveWords', () => {
  const s = { title: 'Fix login', status: 'idle' as const, attachCommand: null };

  it('says where the copy goes on and what becomes of this one', () => {
    expect(moveWords(s, 'c2', 'Ghostty', false)).toBe('A copy of “Fix login” goes on under c2 in a new Ghostty tab, and this one is closed and archived.');
  });

  it('says a background job goes on as one, told to carry on where a limit stopped it', () => {
    expect(moveWords({ ...s, attachCommand: 'claude --attach x' }, 'c2', 'Ghostty', true)).toBe('A copy of “Fix login” goes on under c2 as a background job, told to carry on where it stopped, and this one is closed and archived.');
  });

  it('says a turn under way is stopped first', () => {
    expect(moveWords({ ...s, status: 'working' }, 'c2', 'Ghostty', false)).toMatch(/ Its turn is stopped first\.$/);
  });
});
