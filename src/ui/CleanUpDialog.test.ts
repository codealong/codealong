import { describe, expect, it } from 'vitest';
import type { Removal } from '../api';
import * as fx from '../testing/fixtures';
import { cleanedSummary } from './CleanUpDialog';

const gone = (message: string, noted = false): Removal => ({ removed: true, message, steps: [], noted });

describe('cleanedSummary', () => {
  const a = fx.lane({ id: '/r/a' });
  const b = fx.lane({ id: '/r/b' });

  it('says what was done to one in its own words', () => {
    expect(cleanedSummary([a], [gone('Removed the worktree a and its branch, a.')])).toBe('Removed the worktree a and its branch, a.');
  });

  it('counts several, and says what didn’t go as asked for any that went', () => {
    expect(cleanedSummary([a, b], [gone('Removed the worktree a.'), gone('Removed the worktree b.')])).toBe('Removed 2 worktrees.');
    const half = 'Removed the worktree b through the repo’s WorktreeRemove hook, which failed, though: port 3000 still taken.';
    expect(cleanedSummary([a, b], [gone('Removed the worktree a.'), gone(half, true)])).toBe(`Removed 2 worktrees. ${half}`);
  });

  it('says nothing when none went', () => {
    expect(cleanedSummary([a, b], [{ removed: false, message: 'a stays: 1 uncommitted file.', steps: [] }, null])).toBe('');
  });
});
