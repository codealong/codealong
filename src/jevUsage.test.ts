import { describe, expect, it } from 'vitest';
import { countedSince, jevMoney, jevTotals } from './jevUsage';
import type { JevReport, JevUsed } from './types';

const used = (requests: number, tokens: number): JevUsed => ({ requests, tokens, cost: (tokens * 0.042) / 1e6 });

const report: JevReport = {
  days: [
    { day: '2026-10-14', features: { sessions: used(40, 120_000) } },
    { day: '2026-10-15', features: {} },
    { day: '2026-10-16', features: { quick_panel: used(3, 108_000), sessions: used(10, 30_000), test: used(1, 281) } },
  ],
  since: new Date(2026, 9, 14, 15, 12).getTime(),
};

describe('Jev’s usage', () => {
  it('sums the last days in all and by feature, the most tokens first', () => {
    const today = jevTotals(report, 1);
    expect(today.features.map((f) => f.feature)).toEqual(['quick_panel', 'sessions', 'test']);
    expect(today.total.requests).toBe(14);
    expect(today.total.tokens).toBe(138_281);
    const all = jevTotals(report, 30);
    expect(all.features.map((f) => [f.feature, f.used.requests])).toEqual([
      ['sessions', 50],
      ['quick_panel', 3],
      ['test', 1],
    ]);
    expect(all.total.cost).toBeCloseTo((258_281 * 0.042) / 1e6, 12);
  });

  it('says nothing for a period with no requests', () => {
    const none = jevTotals({ ...report, days: [{ day: '2026-10-16', features: {} }] }, 7);
    expect(none).toEqual({ total: { requests: 0, tokens: 0, cost: 0 }, features: [] });
  });

  it('says when counting started only when that was inside the period', () => {
    // Counting started on the 14th at 3:12 PM: the three days began before it, today after.
    expect(countedSince(report, 7)).toBe(report.since);
    expect(countedSince(report, 1)).toBeNull();
    expect(countedSince({ ...report, since: null }, 7)).toBeNull();
  });

  it('says costs to the hundredth of a cent under a cent', () => {
    expect(jevMoney(0)).toBe('$0');
    expect(jevMoney(0.00294)).toBe('$0.0029');
    expect(jevMoney(0.00001)).toBe('<$0.0001');
    expect(jevMoney(0.009999)).toBe('$0.01');
    expect(jevMoney(0.314)).toBe('$0.31');
  });
});
