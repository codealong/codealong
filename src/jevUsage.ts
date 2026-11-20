// What Settings › Decisions says Codealong's requests to decision models used (ledger.rs
// `JevReport`), worked out as pure functions so tests can pin them: a period's totals, in all, by
// feature and by model; when counting started, if that was inside the period; and costs in words
// fine enough for fractions of a cent.

import { money } from './spend';
import type { JevReport, JevUsed } from './types';

/** The periods the page sums over, in days, today's included. */
export const JEV_SPANS: { id: number; label: string }[] = [
  { id: 1, label: 'Today' },
  { id: 7, label: '7 days' },
  { id: 30, label: '30 days' },
];

const NONE: JevUsed = { requests: 0, tokens: 0, cost: 0 };

function plus(a: JevUsed, b: JevUsed): JevUsed {
  return { requests: a.requests + b.requests, tokens: a.tokens + b.tokens, cost: a.cost + b.cost };
}

/** Each key's sum over `days`, the most tokens first. */
function summed(days: Record<string, JevUsed>[]): { key: string; used: JevUsed }[] {
  const by = new Map<string, JevUsed>();
  for (const d of days) {
    for (const [key, used] of Object.entries(d)) by.set(key, plus(by.get(key) ?? NONE, used));
  }
  return [...by].map(([key, used]) => ({ key, used })).sort((a, b) => b.used.tokens - a.used.tokens || a.key.localeCompare(b.key));
}

/** What the report's last `days` days used, in all, by feature and by model (`<provider>:<model>`),
 *  the most tokens first. */
export function jevTotals(r: JevReport, days: number): { total: JevUsed; features: { feature: string; used: JevUsed }[]; models: { model: string; used: JevUsed }[] } {
  const last = r.days.slice(-days);
  const features = summed(last.map((d) => d.features)).map(({ key, used }) => ({ feature: key, used }));
  const models = summed(last.map((d) => d.models ?? {})).map(({ key, used }) => ({ model: key, used }));
  return { total: features.reduce((t, f) => plus(t, f.used), NONE), features, models };
}

/** When counting started, if that was after the first of the last `days` days began: nothing
 *  before it was kept, so the page says so rather than pass a part of the period off as all of it. */
export function countedSince(r: JevReport, days: number): number | null {
  const first = r.days[Math.max(0, r.days.length - days)]?.day;
  if (r.since == null || !first) return null;
  const [y, m, d] = first.split('-').map(Number);
  return r.since > new Date(y, m - 1, d).getTime() ? r.since : null;
}

/** Dollars fine enough for decision models, whose requests cost hundredths of a cent: `$0.0029` under a cent,
 *  and as the Analytics window says them from a cent up. */
export function jevMoney(n: number): string {
  if (n <= 0) return '$0';
  if (n < 0.0001) return '<$0.0001';
  const fine = n.toFixed(4);
  return n < 0.01 && fine !== '0.0100' ? `$${fine}` : money(n);
}
