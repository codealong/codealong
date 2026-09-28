import { describe, expect, it } from 'vitest';
import type { GitInfo } from '../types';
import { driftWords } from './primitives';

describe('driftWords', () => {
  const git = (g: Partial<GitInfo>): GitInfo => ({ upstream: null, ahead: 0, behind: 0, dirty: 0, lastCommit: null, lastCommitAt: null, added: null, removed: null, base: 'origin/main', baseAhead: 0, baseBehind: 0, ...g });

  it('says how far a worktree has moved from the default branch', () => {
    expect(driftWords(git({ baseAhead: 3, baseBehind: 42 }))).toBe('3 commits ahead of origin/main, 42 behind');
    expect(driftWords(git({ baseAhead: 1 }))).toBe('1 commit ahead of origin/main');
    expect(driftWords(git({ baseBehind: 1200 }))).toBe('1,200 commits behind origin/main');
  });

  it('says nothing when it is even with it or git has not said', () => {
    expect(driftWords(git({}))).toBeNull();
    expect(driftWords(git({ base: undefined, baseAhead: undefined, baseBehind: undefined }))).toBeNull();
    expect(driftWords(null)).toBeNull();
  });
});
