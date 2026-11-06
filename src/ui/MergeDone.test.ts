import { describe, expect, it } from 'vitest';
import * as fx from '../testing/fixtures';
import type { MergeDoneAsk } from '../menus';
import { comesBack, freshAsk, mergeDoneWords } from './MergeDone';

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

describe('freshAsk', () => {
  const session = fx.session({ key: 'claude:c:1', title: 'Fix login' });
  const ask = (number: number, refused?: MergeDoneAsk['refused']): MergeDoneAsk => ({ lane: fx.lane(), session, pr: fx.prItem({ key: `acme/api#${number}`, number }), refused, then: () => {} });
  const refused = { commit: { method: 'squash' as const, title: 'Add login (#7)', body: '', email: null, deleteBranch: false }, why: 'Couldn’t merge #7: Head branch was modified.' };

  it('opens on what a refused merge of the same PR left, once', () => {
    const kept = new Map([[session.key, ask(7, refused)]]);
    expect(freshAsk(ask(7), kept).refused).toEqual(refused);
    expect(kept.size).toBe(0);
    expect(freshAsk(ask(7), kept).refused).toBeUndefined();
  });

  it('leaves one of another PR the session is the primary of for it', () => {
    const kept = new Map([[session.key, ask(7, refused)]]);
    expect(freshAsk(ask(8), kept).refused).toBeUndefined();
    expect(kept.get(session.key)?.pr.key).toBe('acme/api#7');
  });
});
