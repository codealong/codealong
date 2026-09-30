import { describe, expect, it } from 'vitest';
import type { Lane, Session } from '../types';
import { timeSections, waitingQueue, type SessionItem } from './SessionsView';

const item = (key: string, lastActivity: number, status: Session['status'] = 'idle', lastPromptAt = lastActivity, statusSince: number | null = null): SessionItem => ({
  kind: 'agent',
  key: `lane::${key}`,
  lane: {} as Lane,
  session: { key, lastActivity, status, lastPromptAt, statusSince } as Session,
});

describe('timeSections', () => {
  it('groups by local day, newest first', () => {
    const now = new Date(2026, 8, 24, 15, 0).getTime();
    const at = (d: number, h: number) => new Date(2026, 8, d, h, 0).getTime();
    const sections = timeSections([item('old', at(10, 9)), item('today2', at(24, 14)), item('yday', at(23, 23)), item('today1', at(24, 1)), item('week', at(19, 12))], now);
    expect(sections.map((s) => [s.label, s.rows.map((r) => r.session.key)])).toEqual([
      ['Today', ['today2', 'today1']],
      ['Yesterday', ['yday']],
      ['Last 7 days', ['week']],
      ['Older', ['old']],
    ]);
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
    expect(timeSections(rows, now).map((s) => [s.label, s.rows.map((r) => r.session.key)])).toEqual([
      // Needs you and Working the longest first; Your turn the newest status first.
      ['Needs you', ['failed', 'blocked']],
      ['Your turn', ['turn']],
      ['Working', ['busy-old', 'busy-new']],
      ['Today', ['idle']],
      ['Yesterday', ['stopped']],
    ]);
  });

  it('keeps snoozed sessions apart after Working, the soonest back first, unless they run', () => {
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
    expect(timeSections(rows, now).map((s) => [s.label, s.rows.map((r) => r.session.key)])).toEqual([
      ['Needs you', ['blocked']],
      ['Working', ['busy']],
      ['Snoozed', ['soon', 'quiet', 'later']],
      ['Today', ['idle']],
    ]);
  });
});

describe('waitingQueue', () => {
  it('lists what waits on you in the sections\' order, pinned ones too, and leaves snoozed ones to wait', () => {
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
