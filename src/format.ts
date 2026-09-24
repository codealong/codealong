// Times and text as the brand book asks: compact relative times, live durations, middle truncation.

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

/** `now`, `4m`, `2h`, `3d`, `9w`. */
export function ago(ts: number, now: number): string {
  const d = Math.max(0, now - ts);
  if (d < 45_000) return 'now';
  if (d < HOUR) return `${Math.max(1, Math.round(d / MIN))}m`;
  if (d < DAY) return `${Math.floor(d / HOUR)}h`;
  if (d < 100 * DAY) return `${Math.floor(d / DAY)}d`;
  return `${Math.floor(d / (7 * DAY))}w`;
}

/** `just now`, `4m ago`. */
export function agoPhrase(ts: number, now: number): string {
  const a = ago(ts, now);
  return a === 'now' ? 'just now' : `${a} ago`;
}

/** Elapsed time for running work: `<1m`, `14m`, `2h 5m`, `1d 3h`. */
export function duration(from: number, now: number): string {
  const d = Math.max(0, now - from);
  if (d < MIN) return '<1m';
  if (d < HOUR) return `${Math.floor(d / MIN)}m`;
  const h = Math.floor(d / HOUR);
  const m = Math.floor((d % HOUR) / MIN);
  if (d < DAY) return m ? `${h}h ${m}m` : `${h}h`;
  return `${Math.floor(d / DAY)}d ${h % 24}h`;
}

/** Time until a reset, as short as it gets: `<1m`, `42m`, `5h`, `2d`, each to the nearest unit. */
export function untilShort(at: number, now: number): string {
  const d = at - now;
  if (d < MIN) return '<1m';
  if (d < HOUR - MIN / 2) return `${Math.round(d / MIN)}m`;
  if (d < DAY - HOUR / 2) return `${Math.round(d / HOUR)}h`;
  return `${Math.round(d / DAY)}d`;
}

/** When a limit resets: `in 42m`, `in 3h 10m`, then `Fri 10:59 PM` beyond a day. */
export function resetsPhrase(at: number, now: number): string {
  if (at <= now) return 'now';
  if (at - now < DAY) return `in ${duration(now, at)}`;
  return new Date(at).toLocaleString('en-US', { weekday: 'short', hour: 'numeric', minute: '2-digit' });
}

/** Replaces `{ago:<ms>}` and `{for:<ms>}` tokens with live times. */
export function withTimes(text: string, now: number): string {
  return text.replace(/\{(ago|for):(-?\d+)\}/g, (_, kind: string, ms: string) =>
    kind === 'ago' ? agoPhrase(Number(ms), now) : duration(Number(ms), now),
  );
}

export function absolute(ts: number): string {
  return new Date(ts).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
}

/** Shortens in the middle, keeping both ends, because the end is where branches differ. */
export function middleTruncate(s: string, max: number): string {
  if (s.length <= max) return s;
  const keep = max - 1;
  return `${s.slice(0, Math.ceil(keep / 2))}…${s.slice(s.length - Math.floor(keep / 2))}`;
}

/** `/Users/me/projects/x` → `~/projects/x`. */
export function tildePath(p: string): string {
  return p.replace(/^\/Users\/[^/]+(?=\/|$)/, '~');
}

export function count(n: number, word: string): string {
  return `${n.toLocaleString('en-US')} ${word}${n === 1 ? '' : 's'}`;
}
