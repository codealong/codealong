// What the Usage window says of a token report (ledger.rs `Report`), worked out as pure functions
// so tests can pin them: money and token counts in words, what was paid against what the work
// would have cost at API prices, the running totals the chart draws, its axis, the colors of the
// accounts, and the report's folders gathered into the board's projects.

import type { Lane, Session, Spent, SpendDay, TokenCounts, TokenReport } from './types';

/** Dollars as the window says them: cents under $1,000, whole dollars above, and a cost too
 *  small for a cent as such rather than as nothing. */
export function money(n: number): string {
  if (n === 0) return '$0';
  if (n > 0 && n < 0.005) return '<$0.01';
  const whole = Math.abs(n) >= 1000;
  return `${n < 0 ? '−' : ''}$${Math.abs(n).toLocaleString('en-US', { minimumFractionDigits: whole ? 0 : 2, maximumFractionDigits: whole ? 0 : 2 })}`;
}

/** A dollar amount for an axis: $0, $250, $1.5K, $12K. */
export function moneyTick(n: number): string {
  if (n >= 1000) return `$${trim(n / 1000)}K`;
  return `$${trim(n)}`;
}

function trim(n: number): string {
  return Number.isInteger(n) ? String(n) : n.toFixed(n < 10 ? 1 : 0).replace(/\.0$/, '');
}

/** A token count in a few characters: 980, 12.9K, 38.2M, 1.4B. */
export function tokenCount(n: number): string {
  const steps: [number, string][] = [
    [1e9, 'B'],
    [1e6, 'M'],
    [1e3, 'K'],
  ];
  for (const [size, unit] of steps) {
    if (n >= size) {
      const v = n / size;
      return `${v >= 100 ? Math.round(v) : v.toFixed(1).replace(/\.0$/, '')}${unit}`;
    }
  }
  return String(Math.round(n));
}

export function totalTokens(t: TokenCounts): number {
  return t.input + t.cacheRead + t.cacheWrite + t.output;
}

/** How much of what went in was read from the cache, 0 to 1: most of a coding agent's tokens are,
 *  and they cost a tenth of the rest or less, so the window says so beside the count. */
export function cacheShare(t: TokenCounts): number {
  const into = t.input + t.cacheRead + t.cacheWrite;
  return into === 0 ? 0 : t.cacheRead / into;
}

/** What the work would have cost at API prices for each dollar paid: 1.9×. None while nothing
 *  was paid that Codealong knows of. */
export function leverage(api: number, paid: number): string | null {
  if (paid <= 0 || api <= 0) return null;
  const x = api / paid;
  return `${x >= 10 ? Math.round(x) : x.toFixed(1)}×`;
}

/** The running totals at the end of each day, at API prices and paid: what the chart's lines
 *  draw. */
export function cumulative(days: SpendDay[]): { day: string; api: number; paid: number }[] {
  let api = 0;
  let paid = 0;
  return days.map((d) => {
    api += Object.values(d.api).reduce((a, b) => a + b, 0);
    paid += d.paid;
    return { day: d.day, api, paid };
  });
}

/** Round steps for an axis from 0 past `max`, about `count` of them: 0, 250, 500, 750. */
export function ticks(max: number, count = 4): number[] {
  if (max <= 0) return [0, 1];
  const rough = max / count;
  const pow = 10 ** Math.floor(Math.log10(rough));
  const step = [1, 2, 2.5, 5, 10].map((m) => m * pow).find((s) => s >= rough) ?? 10 * pow;
  const out: number[] = [];
  for (let v = 0; v < max + step; v += step) out.push(Math.round(v * 1e6) / 1e6);
  return out;
}

/** How many accounts get a color of their own; the rest are Other. */
export const SERIES = 6;

/** The color each account's bars wear, `--chart-N`, by its place among the accounts in config.toml
 *  so it keeps it whatever the report shows; past `SERIES` they share Other's. */
export function seriesColor(account: string, order: string[]): string {
  const i = order.indexOf(account);
  return i >= 0 && i < SERIES ? `var(--chart-${i + 1})` : 'var(--line-strong)';
}

/** The accounts a day's bars stack, in config order, and Other for the rest. */
export function stackOf(day: SpendDay, order: string[]): { account: string; api: number }[] {
  const shown = order.slice(0, SERIES);
  const out = shown.map((account) => ({ account, api: day.api[account] ?? 0 }));
  const other = Object.entries(day.api)
    .filter(([a]) => !shown.includes(a))
    .reduce((sum, [, v]) => sum + v, 0);
  if (other > 0) out.push({ account: 'Other', api: other });
  return out.filter((s) => s.api > 0);
}

/** `2026-10-12` as the chart's axis says it: Oct 12. */
export function dayLabel(day: string): string {
  const [y, m, d] = day.split('-').map(Number);
  return new Date(y, m - 1, d).toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
}

/** The board's worktree a folder is in: the lane whose path holds it, the longest such. */
export function laneOf(folder: string, lanes: Lane[]): Lane | null {
  let best: Lane | null = null;
  for (const l of lanes) {
    if (l.device) continue;
    if ((folder === l.path || folder.startsWith(`${l.path}/`)) && (!best || l.path.length > best.path.length)) best = l;
  }
  return best;
}

/** A folder's last two parts, for one in no project of yours: projects/app. */
export function shortFolder(folder: string): string {
  const parts = folder.split('/').filter(Boolean);
  return parts.length === 0 ? 'Unknown folder' : parts.slice(-2).join('/');
}

/** The report's folders gathered into the board's projects, costliest first. A folder in none
 *  is one of its own. */
export function byProject(folders: Spent[], lanes: Lane[]): Spent[] {
  const out = new Map<string, Spent>();
  for (const f of folders) {
    const lane = laneOf(f.key, lanes);
    const key = lane ? lane.project : f.key ? shortFolder(f.key) : 'Unknown folder';
    const had = out.get(key);
    if (!had) {
      out.set(key, { ...f, key, tokens: { ...f.tokens } });
      continue;
    }
    had.api += f.api;
    had.paid += f.paid;
    had.calls += f.calls;
    had.unpriced += f.unpriced;
    had.lastAt = Math.max(had.lastAt, f.lastAt);
    for (const k of ['input', 'cacheRead', 'cacheWrite', 'output', 'reasoning'] as const) had.tokens[k] += f.tokens[k];
  }
  return [...out.values()].sort((a, b) => b.api - a.api || totalTokens(b.tokens) - totalTokens(a.tokens));
}

/** A session's row as the window names it: its title on the board, else its folder. */
export function sessionLabel(s: Spent, sessions: Map<string, Session>): string {
  const on = sessions.get(s.key);
  if (on) return on.title || on.excerpt || on.shortId;
  return s.folder ? shortFolder(s.folder) : s.key.split(':').pop()?.slice(0, 8) ?? s.key;
}

/** What the window says it has, under its numbers: when it last read, and whether it's still
 *  reading the older records it found. */
export function readNote(r: TokenReport): string | null {
  if (r.filling) return 'Reading your sessions’ records for the first time; older days fill in as it goes.';
  return null;
}
