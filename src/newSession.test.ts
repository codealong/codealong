import { describe, expect, it } from 'vitest';
import { forOpen, listedIn, newSessionAfter, newSessionDraft, type NewSessionShown } from './newSession';
import * as fx from './testing/fixtures';

const shown = (over: string | null): NewSessionShown => ({
  draft: { spec: { laneId: 'l1', account: 'work', place: 'background', prompt: '', model: null, effort: null, worktree: false, todos: [] }, does: 'Starts work in feature in the background.', placeLabel: 'Background', harness: 'claude' },
  over,
  opened: 1,
  sending: null,
  error: null,
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

  it('takes what a start answers only for the open that sent it', () => {
    const n = shown('l1::a');
    const sent = forOpen(n, 1, (x) => ({ ...x, sending: 'Fix it' }));
    expect(sent?.sending).toBe('Fix it');
    // Another opened since: a late answer leaves it as it is, and closes nothing.
    const newer = { ...n, opened: 2 };
    expect(forOpen(newer, 1, () => null)).toBe(newer);
    expect(forOpen(newer, 1, (x) => ({ ...x, error: 'late' }))).toBe(newer);
    expect(forOpen(null, 1, () => n)).toBeNull();
    expect(forOpen(n, 1, () => null)).toBeNull();
  });

  it('finds the worktree that lists the session a send started', () => {
    const started = fx.session({ key: 'claude:work:abc' });
    const home = fx.lane({ id: 'l2', sessions: [started] });
    const lanes = [fx.lane({ id: 'l1', sessions: [fx.session({ key: 'claude:work:other' })] }), home];
    expect(listedIn(lanes, 'claude:work:abc')).toBe(home);
    expect(listedIn(lanes, 'claude:work:not-yet')).toBeNull();
  });
});
