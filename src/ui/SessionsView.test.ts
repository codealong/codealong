import { describe, expect, it } from 'vitest';
import type { Lane, Session } from '../types';
import { timeSections, type SessionItem } from './SessionsView';

const item = (key: string, lastActivity: number, status: Session['status'] = 'idle', lastPromptAt = lastActivity): SessionItem => ({
  kind: 'agent',
  key: `lane::${key}`,
  lane: {} as Lane,
  session: { key, lastActivity, status, lastPromptAt } as Session,
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

  it('puts the most urgent first within a day, then the one you last sent a prompt', () => {
    const now = new Date(2026, 8, 24, 15, 0).getTime();
    const at = (h: number, m = 0) => new Date(2026, 8, 24, h, m).getTime();
    const rows = [
      item('idle', at(14, 59)),
      item('busy-old-ask', at(14, 59), 'working', at(9)),
      item('busy-new-ask', at(14, 30), 'working', at(14)),
      item('blocked', at(12), 'needs', at(11)),
    ];
    expect(timeSections(rows, now)[0].rows.map((r) => r.session.key)).toEqual(['blocked', 'busy-new-ask', 'busy-old-ask', 'idle']);
  });
});
