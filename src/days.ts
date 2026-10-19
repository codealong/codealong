// The day headings a list sorted by time goes under (Sessions by start or last activity, PRs by
// when they were opened, todos by when they were added): Today, Yesterday, Last 7 days and Older,
// in local time.

export type DayId = 'today' | 'yesterday' | 'week' | 'older';

export const DAYS: { id: DayId; label: string }[] = [
  { id: 'today', label: 'Today' },
  { id: 'yesterday', label: 'Yesterday' },
  { id: 'week', label: 'Last 7 days' },
  { id: 'older', label: 'Older' },
];

/** `rows`, already in order, under each day heading by `at`; a heading with none under it is
 *  left in, with no rows, for the caller to drop. */
export function byDay<T>(rows: T[], at: (r: T) => number, now: number): { id: DayId; label: string; rows: T[] }[] {
  const today = new Date(now);
  today.setHours(0, 0, 0, 0);
  const daysBack = (n: number) => {
    const d = new Date(today);
    d.setDate(d.getDate() - n);
    return d.getTime();
  };
  const [t0, t1, t6] = [today.getTime(), daysBack(1), daysBack(6)];
  const day = (r: T) => {
    const t = at(r);
    return t >= t0 ? 0 : t >= t1 ? 1 : t >= t6 ? 2 : 3;
  };
  return DAYS.map((d, i) => ({ ...d, rows: rows.filter((r) => day(r) === i) }));
}

/** The newest first, by `at`, then by `key` so equal times keep one order. */
export function newestFirst<T>(rows: T[], at: (r: T) => number, key: (r: T) => string): T[] {
  return [...rows].sort((a, b) => at(b) - at(a) || key(a).localeCompare(key(b)));
}
