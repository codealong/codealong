// The words and sums for the Analytics window's Your part and Sessions (ui/UsageAnalytics.tsx):
// what came in to your sessions and who from, what your prompts did, how sessions began and how
// you left them (analytics.rs). Pure, so the tests pin them.

import type { Count, Outcomes, Part, PartDay, PartRow, Spent } from './types';

/** A series of a chart: its key in the report, what it's called, and the color its marks wear.
 *  The order is fixed and follows the thing, never its size (`--chart-N`'s own rule). */
export interface Series {
  id: string;
  label: string;
  color: string;
}

/** What a prompt of yours did with what the agent had last said (stance.rs `STANCES`, and a
 *  slash command), in the order the charts stack them; the ones nothing has read come last, in
 *  the color charts give Other. */
export const STANCES: Series[] = [
  { id: 'sets-out', label: 'Set out new work', color: 'var(--chart-1)' },
  { id: 'agrees', label: 'Agreed', color: 'var(--chart-2)' },
  { id: 'steers', label: 'Steered', color: 'var(--chart-3)' },
  { id: 'redirects', label: 'Redirected', color: 'var(--chart-4)' },
  { id: 'asks', label: 'Asked something', color: 'var(--chart-5)' },
  { id: 'command', label: 'Ran a command', color: 'var(--chart-6)' },
  { id: '', label: 'Not read', color: 'var(--line-strong)' },
];

/** Who began a turn (turns.rs `Source::word`), as what it counts as: a turn the session began
 *  itself is counted with the one it follows. */
export const HANDS: Series[] = [
  { id: 'you', label: 'You', color: 'var(--chart-1)' },
  { id: 'agent', label: 'Other agents', color: 'var(--chart-2)' },
  { id: 'codealong', label: 'Codealong', color: 'var(--chart-3)' },
  { id: '', label: 'Not known', color: 'var(--line-strong)' },
];

/** Who something that came in was from, as its record says: the session's own doing apart. */
export const SOURCES: Series[] = [HANDS[0], HANDS[1], HANDS[2], { id: 'itself', label: 'The session itself', color: 'var(--chart-4)' }];

/** How a session began (began.rs `Began::word`). */
export const BEGAN: Series[] = [
  { id: 'you', label: 'You', color: 'var(--chart-1)' },
  { id: 'fork', label: 'You, as a copy of another', color: 'var(--chart-1)' },
  { id: 'helper', label: 'An agent, as its helper', color: 'var(--chart-2)' },
  { id: 'agent', label: 'An agent', color: 'var(--chart-2)' },
  { id: 'review', label: 'Codealong, to review a PR', color: 'var(--chart-3)' },
  { id: 'schedule', label: 'Codealong, on a schedule', color: 'var(--chart-3)' },
  { id: 'headless', label: 'A headless run', color: 'var(--chart-4)' },
  { id: '', label: 'Not known', color: 'var(--line-strong)' },
];

/** How the sessions you were done with stood. */
export const STOOD: Series[] = [
  { id: 'finished', label: 'Finished', color: 'var(--chart-1)' },
  { id: 'lookedFinished', label: 'Looked finished', color: 'var(--chart-2)' },
  { id: 'unfinished', label: 'Unfinished', color: 'var(--chart-3)' },
  { id: 'unknown', label: 'Not known', color: 'var(--line-strong)' },
];

/** The beginnings that are yours: the rest are agents' or Codealong's. */
const YOURS = new Set(['you', 'fork']);

export function sum(values: Iterable<number>): number {
  let n = 0;
  for (const v of values) n += v;
  return n;
}

/** How many a list of counts has under a key. */
export function countOf(list: Count[], key: string): number {
  return list.find((c) => c.key === key)?.n ?? 0;
}

/** A share as a whole percent, "—" of nothing. Under one percent of something is "<1%", never
 *  "0%": it happened. */
export function percent(n: number, of: number): string {
  if (of <= 0) return '—';
  const p = (n / of) * 100;
  if (n > 0 && p < 1) return '<1%';
  return `${Math.round(p)}%`;
}

/** A count with thousands separated. */
export function count(n: number): string {
  return n.toLocaleString('en-US');
}

/** The marks of a chart's count axis, from none to at least `max`: whole numbers, in steps of
 *  1, 2 or 5 times a power of ten, about `about` of them. */
export function countTicks(max: number, about = 3): number[] {
  const rough = Math.max(1, max) / about;
  const pow = 10 ** Math.floor(Math.log10(rough));
  const step = Math.max(1, [1, 2, 5, 10].map((m) => m * pow).find((s) => s >= rough) ?? 10 * pow);
  const out: number[] = [];
  for (let v = 0; v < Math.max(1, max) + step; v += step) out.push(v);
  return out;
}

/** A segment of a stack: a series and how much of it there is. */
export interface Segment extends Series {
  n: number;
}

/** The series that have something, in their own order; a key the series don't name goes last,
 *  under its own word, so nothing counted is left out of the picture. */
export function segments(series: Series[], by: Record<string, number>): Segment[] {
  const known = series.map((s) => ({ ...s, n: by[s.id] ?? 0 }));
  const names = new Set(series.map((s) => s.id));
  const others = Object.entries(by)
    .filter(([k]) => !names.has(k))
    .map(([k, n]) => ({ id: k, label: k, color: 'var(--line-strong)', n }));
  return [...known, ...others].filter((s) => s.n > 0);
}

export function byKey(list: Count[]): Record<string, number> {
  return Object.fromEntries(list.map((c) => [c.key, c.n]));
}

/** A day's prompts of yours, stacked by what they did. */
export function stanceStack(day: PartDay): Segment[] {
  return segments(STANCES, day.stances);
}

/** Of your prompts that were read for what they did with a reply, how many went each way:
 *  agreed, steered, redirected. Opening new work, asking and commands answer no reply, and the
 *  unread say nothing yet. */
export function responses(stances: Count[]): { agreed: number; steered: number; redirected: number; all: number } {
  const [agreed, steered, redirected] = ['agrees', 'steers', 'redirects'].map((k) => countOf(stances, k));
  return { agreed, steered, redirected, all: agreed + steered + redirected };
}

/** What the turns you started came to, at API prices, against all of it whose turn is known:
 *  your share of the cost. Null when nothing's known. */
export function yourCost(hands: Spent[] | undefined): { yours: number; known: number } | null {
  if (!hands?.length) return null;
  const known = sum(hands.filter((h) => h.key !== '').map((h) => h.api));
  if (known <= 0) return null;
  return { yours: sum(hands.filter((h) => h.key === 'you').map((h) => h.api)), known };
}

/** Sessions you started, a copy you made of one among them, against the ones whose beginning is
 *  known. */
export function yourSessions(began: Count[]): { yours: number; known: number } {
  const known = began.filter((b) => b.key !== '');
  return { yours: sum(known.filter((b) => YOURS.has(b.key)).map((b) => b.n)), known: sum(known.map((b) => b.n)) };
}

/** A row's prompts of yours: every one has a stance, read or not. Not `counts.you`, which also
 *  has the turns the session began itself after a prompt of yours. */
export function yours(row: PartRow): number {
  return sum(Object.values(row.stances));
}

/** Every turn of a row's sessions. */
export function cameIn(row: PartRow): number {
  return sum(Object.values(row.counts));
}

/** The folders' rows summed by project, as the Cost table sums its folders (`spend.ts`
 *  `byProject`): `projectOf` names a folder's project, "" for none. */
export function partByProject(folders: PartRow[], projectOf: (folder: string) => string): PartRow[] {
  const out = new Map<string, PartRow>();
  for (const f of folders) {
    const key = projectOf(f.key) || 'No project';
    const row = out.get(key) ?? { key, counts: {}, stances: {}, lastAt: 0 };
    for (const [k, n] of Object.entries(f.counts)) row.counts[k] = (row.counts[k] ?? 0) + n;
    for (const [k, n] of Object.entries(f.stances)) row.stances[k] = (row.stances[k] ?? 0) + n;
    row.lastAt = Math.max(row.lastAt, f.lastAt);
    out.set(key, row);
  }
  return [...out.values()].sort((a, b) => yours(b) - yours(a) || b.lastAt - a.lastAt);
}

/** What the marks say in a sentence each, the ones with nothing to say left out. */
export function markLines(part: Part): string[] {
  const m = part.marks;
  const s = (n: number, one: string, many: string) => `${count(n)} ${n === 1 ? one : many}`;
  const answers = m.pickedFirst + m.pickedOffered + m.ownWords;
  return [
    m.mid > 0 && `${count(m.mid)} sent while the agent was working`,
    m.afterStop > 0 && `${count(m.afterStop)} sent after you stopped it`,
    answers > 0 && `${s(answers, 'answer', 'answers')} to the agent’s questions: ${count(m.pickedFirst)} its first option, ${count(m.pickedOffered)} another option, ${count(m.ownWords)} in your own words`,
    m.allowed + m.denied > 0 && `${s(m.allowed, 'tool call', 'tool calls')} allowed and ${count(m.denied)} denied in Codealong`,
    m.suggested > 0 && `${s(m.suggested, 'suggested reply', 'suggested replies')} sent unchanged`,
  ].filter((l): l is string => !!l);
}

/** How the sessions you were done with stood, as a stack. */
export function stoodStack(o: Outcomes): Segment[] {
  return segments(STOOD, { finished: o.finished, lookedFinished: o.lookedFinished, unfinished: o.unfinished, unknown: o.unknown });
}
