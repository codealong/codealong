import { describe, expect, it } from 'vitest';
import * as fx from '../testing/fixtures';
import { comesBack, mergeDoneWords } from './MergeDone';

describe('mergeDoneWords', () => {
  const pr = fx.prItem({ number: 7, base: 'main' });
  const session = fx.session({ title: 'Fix login', live: true, pid: 42 });

  it('says it merges into the base, then what Mark as done does', () => {
    expect(mergeDoneWords({ pr, session })).toBe('Merges #7 into main, then marks “Fix login” done: it closes where Codealong can close it, and is marked seen and archived.');
  });

  it('says work under way ends with it', () => {
    expect(mergeDoneWords({ pr, session: { ...session, status: 'working' } })).toMatch(/ Its turn is stopped first\.$/);
    expect(mergeDoneWords({ pr, session: { ...session, background: 'gh pr checks 7 --watch' } })).toMatch(/ `gh pr checks 7 --watch`, which it left running in the background, stops with it\.$/);
    expect(mergeDoneWords({ pr, session: { ...session, background: ' ' } })).toMatch(/ The command it left running in the background stops with it\.$/);
  });

  it('leaves out a base it doesn’t know', () => {
    expect(mergeDoneWords({ pr: { ...pr, base: null }, session })).toMatch(/^Merges #7, then/);
  });
});

describe('comesBack', () => {
  it('brings a refused merge’s dialog back over the main window, or on a card on its own session', () => {
    expect(comesBack('a', { up: true, open: false })).toBe(true);
    expect(comesBack('a', { up: true, open: false, showing: 'a' })).toBe(true);
  });

  it('keeps it for its session where it would take another’s place', () => {
    // A card gone on to another session, or away; another dialog up, with what you wrote in it.
    expect(comesBack('a', { up: true, open: false, showing: 'b' })).toBe(false);
    expect(comesBack('a', { up: false, open: false, showing: 'a' })).toBe(false);
    expect(comesBack('a', { up: true, open: true })).toBe(false);
  });
});
