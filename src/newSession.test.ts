import { describe, expect, it } from 'vitest';
import { listedIn, newSessionAfter, newSessionDraft, type NewSessionShown } from './newSession';
import * as fx from './testing/fixtures';

const shown = (over: string | null): NewSessionShown => ({
  draft: { spec: { laneId: 'l1', account: 'work', place: 'background', prompt: '', model: null, effort: null, worktree: false, todos: [] }, does: 'Starts work in feature in the background.', placeLabel: 'Background', harness: 'claude' },
  over,
  opened: 1,
  started: null,
});

describe('a new session waiting for its first prompt', () => {
  it('stays over the row it opened over, and goes once another is picked', () => {
    const n = shown('l1::a');
    expect(newSessionAfter(n, 'l1::a')).toBe(n);
    expect(newSessionAfter(n, 'l1::b')).toBeNull();
    expect(newSessionAfter(n, null)).toBeNull();
    expect(newSessionAfter(null, 'l1::a')).toBeNull();
  });

  it('takes the row the window selects by itself when it opened over none', () => {
    const n = shown(null);
    expect(newSessionAfter(n, 'l1::top')).toEqual({ ...n, over: 'l1::top' });
    expect(newSessionAfter(newSessionAfter(n, 'l1::top'), 'l1::other')).toBeNull();
  });

  it('keeps what you write per worktree, whichever account it starts on', () => {
    expect(newSessionDraft({ laneId: 'l1' })).toBe('new:l1');
    const spec = shown(null).draft.spec;
    expect(newSessionDraft(spec)).toBe(newSessionDraft({ ...spec, account: 'other' } as typeof spec));
  });

  it('finds the worktree that lists the session a send started', () => {
    const started = fx.session({ key: 'claude:work:abc' });
    const home = fx.lane({ id: 'l2', sessions: [started] });
    const lanes = [fx.lane({ id: 'l1', sessions: [fx.session({ key: 'claude:work:other' })] }), home];
    expect(listedIn(lanes, 'claude:work:abc')).toBe(home);
    expect(listedIn(lanes, 'claude:work:not-yet')).toBeNull();
  });
});
