import { describe, expect, it } from 'vitest';
import * as fx from '../testing/fixtures';
import type { Session } from '../types';
import { sortSessions, waitingQueue, type SessionItem } from './SessionsView';

const item = (key: string, lastActivity: number, status: Session['status'] = 'idle', lastPromptAt = lastActivity, statusSince: number | null = null, startedAt: number | null = null): SessionItem => ({
  kind: 'agent',
  key: `lane::${key}`,
  lane: fx.lane(),
  session: fx.session({ key, lastActivity, status, lastPromptAt, statusSince, startedAt }),
});

const keys = (rows: SessionItem[]) => rows.map((r) => r.session.key);

describe('sortSessions by status', () => {
  it('orders the rest by local day, newest first', () => {
    const now = new Date(2026, 8, 24, 15, 0).getTime();
    const at = (d: number, h: number) => new Date(2026, 8, d, h, 0).getTime();
    const rows = [item('old', at(10, 9)), item('today2', at(24, 14)), item('yday', at(23, 23)), item('today1', at(24, 1)), item('week', at(19, 12))];
    expect(keys(sortSessions(rows, 'status', now))).toEqual(['today2', 'today1', 'yday', 'week', 'old']);
  });

  it('keeps a day ahead of an older prompt: last active today comes before prompted yesterday', () => {
    const now = new Date(2026, 8, 24, 15, 0).getTime();
    const at = (d: number, h: number) => new Date(2026, 8, d, h, 0).getTime();
    const rows = [item('yday', at(23, 20), 'idle', at(23, 20)), item('woke', at(24, 9), 'idle', at(20, 9))];
    expect(keys(sortSessions(rows, 'status', now))).toEqual(['woke', 'yday']);
  });

  it("puts what needs you, your turns and working agents first, in the bar's order", () => {
    const now = new Date(2026, 8, 24, 15, 0).getTime();
    const at = (d: number, h: number) => new Date(2026, 8, d, h, 0).getTime();
    const rows = [
      item('idle', at(24, 14)),
      item('busy-old', at(24, 14), 'working', at(24, 9), at(24, 9)),
      item('busy-new', at(24, 12), 'working', at(24, 11), at(24, 11)),
      item('turn', at(20, 10), 'unread'),
      item('blocked', at(24, 12), 'needs', at(24, 11), at(24, 12)),
      item('failed', at(23, 8), 'error', at(23, 8), at(23, 8)),
      item('stopped', at(23, 9), 'stopped'),
    ];
    // Needs you and Working the longest first; Your turn the newest status first.
    expect(keys(sortSessions(rows, 'status', now))).toEqual(['failed', 'blocked', 'turn', 'busy-old', 'busy-new', 'idle', 'stopped']);
  });

  it('keeps snoozed sessions after Working, the soonest back first, unless they run', () => {
    const now = new Date(2026, 8, 24, 15, 0).getTime();
    const at = (d: number, h: number) => new Date(2026, 8, d, h, 0).getTime();
    const snoozed = (r: SessionItem, until: number): SessionItem => ({ ...r, session: { ...r.session, snoozedUntil: until } });
    const rows = [
      item('blocked', at(24, 12), 'needs', at(24, 11), at(24, 12)),
      snoozed(item('later', at(24, 12), 'needs', at(24, 11), at(24, 12)), at(24, 18)),
      snoozed(item('soon', at(24, 13), 'unread'), at(24, 16)),
      snoozed(item('quiet', at(20, 9)), at(24, 17)),
      snoozed(item('busy', at(24, 14), 'working', at(24, 14), at(24, 14)), at(24, 16)),
      item('idle', at(24, 14)),
    ];
    expect(keys(sortSessions(rows, 'status', now))).toEqual(['blocked', 'busy', 'soon', 'quiet', 'later', 'idle']);
  });
});

describe('sortSessions by start', () => {
  it('lists the newest start first, whatever the status, snoozed ones in place too', () => {
    const now = new Date(2026, 8, 24, 15, 0).getTime();
    const at = (d: number, h: number) => new Date(2026, 8, d, h, 0).getTime();
    const late = item('snoozed', at(24, 12), 'unread', at(24, 12), null, at(23, 12));
    const rows = [
      item('old-busy', at(24, 14), 'working', at(24, 14), at(24, 14), at(20, 9)),
      item('new-idle', at(24, 13), 'idle', at(24, 13), null, at(24, 13)),
      item('blocked', at(24, 12), 'needs', at(24, 11), at(24, 12), at(22, 9)),
      { ...late, session: { ...late.session, snoozedUntil: at(24, 18) } },
    ];
    expect(keys(sortSessions(rows, 'started', now))).toEqual(['new-idle', 'snoozed', 'blocked', 'old-busy']);
  });

  it("goes by the last prompt where a session's start isn't known", () => {
    const now = new Date(2026, 8, 24, 15, 0).getTime();
    const at = (d: number, h: number) => new Date(2026, 8, d, h, 0).getTime();
    const rows = [item('known', at(24, 14), 'idle', at(24, 14), null, at(22, 9)), item('unknown', at(24, 10), 'idle', at(23, 9))];
    expect(keys(sortSessions(rows, 'started', now))).toEqual(['unknown', 'known']);
  });
});

describe('waitingQueue', () => {
  it('lists what waits on you in the status order, pinned ones too, and leaves snoozed ones to wait', () => {
    const at = (d: number, h: number) => new Date(2026, 8, d, h, 0).getTime();
    const rows = [
      item('idle', at(24, 14)),
      item('busy', at(24, 12), 'working', at(24, 11), at(24, 11)),
      item('turn-old', at(24, 9), 'unread', at(24, 9), at(24, 9)),
      item('turn-new', at(24, 13), 'unread', at(24, 13), at(24, 13)),
      item('blocked', at(24, 12), 'needs', at(24, 11), at(24, 12)),
      item('failed', at(23, 8), 'error', at(23, 8), at(23, 8)),
    ];
    const later = item('later', at(24, 12), 'needs', at(24, 11), at(24, 10));
    rows.push({ ...later, session: { ...later.session, snoozedUntil: at(24, 18) } });
    expect(waitingQueue(rows).map((r) => r.session.key)).toEqual(['failed', 'blocked', 'turn-new', 'turn-old']);
  });
});
