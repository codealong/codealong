import { describe, expect, it } from 'vitest';
import { byDay, newestFirst } from './days';

describe('byDay', () => {
  // Noon on a Wednesday, local time.
  const now = new Date(2026, 9, 14, 12).getTime();
  const at = (days: number, hour: number) => new Date(2026, 9, 14 - days, hour).getTime();

  it('puts each row under Today, Yesterday, Last 7 days or Older, by local day, keeping its order', () => {
    const rows = [
      { id: 'a', t: at(0, 1) },
      { id: 'b', t: at(1, 23) },
      { id: 'c', t: at(0, 0) },
      { id: 'd', t: at(6, 0) },
      { id: 'e', t: at(7, 23) },
    ];
    expect(byDay(rows, (r) => r.t, now).map((d) => [d.id, d.label, d.rows.map((r) => r.id)])).toEqual([
      ['today', 'Today', ['a', 'c']],
      ['yesterday', 'Yesterday', ['b']],
      ['week', 'Last 7 days', ['d']],
      ['older', 'Older', ['e']],
    ]);
  });
});

describe('newestFirst', () => {
  it('puts the newest first, equal times in one order', () => {
    const rows = [
      { k: 'b', t: 1 },
      { k: 'a', t: 1 },
      { k: 'c', t: 3 },
    ];
    expect(newestFirst(rows, (r) => r.t, (r) => r.k).map((r) => r.k)).toEqual(['c', 'a', 'b']);
  });
});
