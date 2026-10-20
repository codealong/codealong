import { describe, expect, it } from 'vitest';
import * as fx from '../testing/fixtures';
import { mergeCloseWords } from './MergeClose';

describe('mergeCloseWords', () => {
  const pr = fx.prItem({ number: 7, base: 'main' });
  const session = fx.session({ title: 'Fix login', live: true, pid: 42 });

  it('says it merges into the base, then what Mark as done does', () => {
    expect(mergeCloseWords({ pr, session })).toBe('Merges #7 into main, then marks “Fix login” done: it closes where Codealong can close it, and is marked seen and archived.');
  });

  it('says work under way ends with it', () => {
    expect(mergeCloseWords({ pr, session: { ...session, status: 'working' } })).toMatch(/ Its turn is stopped first\.$/);
    expect(mergeCloseWords({ pr, session: { ...session, background: 'gh pr checks 7 --watch' } })).toMatch(/ `gh pr checks 7 --watch`, which it left running in the background, stops with it\.$/);
    expect(mergeCloseWords({ pr, session: { ...session, background: ' ' } })).toMatch(/ The command it left running in the background stops with it\.$/);
  });

  it('leaves out a base it doesn’t know', () => {
    expect(mergeCloseWords({ pr: { ...pr, base: null }, session })).toMatch(/^Merges #7, then/);
  });
});
