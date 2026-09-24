import { describe, expect, it } from 'vitest';
import type { Lane, Session } from '../types';
import { reorderPinned, timeSections, type SessionItem } from './SessionsView';

const item = (key: string, lastActivity: number): SessionItem => ({
  kind: 'agent',
  key: `lane::${key}`,
  lane: {} as Lane,
  session: { key, lastActivity } as Session,
});

describe('reorderPinned', () => {
  it('moves a pin to an insertion point', () => {
    const all = ['a', 'b', 'c'];
    expect(reorderPinned(all, all, 'a', 2)).toEqual(['b', 'a', 'c']);
    expect(reorderPinned(all, all, 'c', 0)).toEqual(['c', 'a', 'b']);
    expect(reorderPinned(all, all, 'b', 3)).toEqual(['a', 'c', 'b']);
  });

  it('leaves the list alone for a move to where it already is', () => {
    const all = ['a', 'b', 'c'];
    expect(reorderPinned(all, all, 'a', -1)).toBe(all);
    expect(reorderPinned(all, all, 'b', 1)).toBe(all);
    expect(reorderPinned(all, all, 'b', 2)).toBe(all);
    expect(reorderPinned(all, all, 'c', 9)).toBe(all);
  });

  it('keeps pins hidden by the project or filter in their places', () => {
    expect(reorderPinned(['a', 'x', 'b', 'c'], ['a', 'b', 'c'], 'c', 0)).toEqual(['c', 'x', 'a', 'b']);
  });
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
});
