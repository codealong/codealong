import { describe, expect, it } from 'vitest';
import type { CloseOutcome, IdleEnds, IdleSession } from '../types';
import { actionLabel, closedSummary, keptSummary } from './CloseIdleDialog';

const row = (key: string, ends: IdleEnds): IdleSession => ({ key, title: key, account: 'claude', harness: 'claude', ends, pid: null, cwd: '/w', worktree: '/w', callsign: 'w', lastActivity: 0, why: 'idle 2 days' });
const out = (ends: IdleEnds | undefined, closed: boolean, archived: boolean): CloseOutcome => ({ key: 'k', title: 'T', ends, closed, archived, ok: closed || archived, message: '' });

describe('keptSummary', () => {
  it('counts what stays open by reason, the recently active together', () => {
    const kept = ['working', 'active 3 hours ago', 'active an hour ago', 'in the Claude app'].map((why) => ({ key: why, title: why, account: 'claude', why }));
    expect(keptSummary(kept)).toBe('1 working, 2 active recently, 1 in the Claude app');
  });
});

describe('actionLabel', () => {
  it('says whether the ticked rows close, close and are archived, or are only archived', () => {
    expect(actionLabel([row('a', 'terminal'), row('b', 'job')], false)).toBe('Close 2 sessions');
    expect(actionLabel([row('a', 'terminal'), row('b', 'archive')], true)).toBe('Close and archive 2 sessions');
    expect(actionLabel([row('b', 'archive')], true)).toBe('Archive 1 session');
    expect(actionLabel([], true)).toBe('Close and archive 0 sessions');
  });
});

describe('closedSummary', () => {
  it('says what closed, what stopped and what was archived', () => {
    expect(closedSummary([out('terminal', true, false), out('terminal', true, false)])).toBe('Closed 2 sessions.');
    expect(closedSummary([out('terminal', true, true), out('job', true, true), out('archive', false, true)])).toBe('Closed 1 session, stopped 1 background job and archived 3.');
    expect(closedSummary([out('job', true, false), out('job', true, false)])).toBe('Stopped 2 background jobs.');
    expect(closedSummary([out('archive', false, true)])).toBe('Archived 1 session.');
    // What didn't go isn't counted.
    expect(closedSummary([out('terminal', false, false), out(undefined, false, false)])).toBe('');
  });
});
