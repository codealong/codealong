// The limit resets an account holds, as the engine reads them (resets.rs): which accounts the
// usage bar marks, and the words its card uses.
import { count, duration } from './format';
import type { Meter, Resets } from './types';

const DAY = 24 * 60 * 60 * 1000;

/** An account's reading, when it's been read. */
export function resetsOf(list: Resets[] | undefined, account: string): Resets | undefined {
  return list?.find((r) => r.account === account);
}

/** The bar marks it: it holds a reset. */
export function holds(r: Resets | undefined): r is Resets {
  return !!r && r.available > 0;
}

/** Its card offers Use a reset: Codealong can use one, and knows which (a use names it). One the
 *  provider only counts, its card says why not. */
export function usable(r: Resets | undefined): boolean {
  return holds(r) && r.canUse && r.credits.length > 0;
}

/** Its heading in the chip's card: `3 limit resets`. */
export function resetsTitle(r: Resets): string {
  return count(r.available, 'limit reset');
}

/** When a reset can no longer be used: `in 5h` within a day, else `Nov 30`. */
export function expiryPhrase(at: number, now: number): string {
  if (at <= now) return 'now';
  if (at - now < DAY) return `in ${duration(now, at)}`;
  return new Date(at).toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
}

/** What they are and when the first runs out: `Full resets · the first expires Nov 30`. */
export function heldNote(r: Resets, now: number): string {
  const titles = [...new Set(r.credits.map((c) => c.title).filter((t): t is string => !!t))];
  const kind = titles.length === 1 ? (r.available === 1 ? titles[0] : `${titles[0]}s`) : null;
  const first = r.credits.find((c) => c.expiresAt != null)?.expiresAt;
  const expires = first == null ? null : `${r.available === 1 ? 'expires' : 'the first expires'} ${expiryPhrase(first, now)}`;
  const said = [kind, expires].filter((s): s is string => !!s).join(' · ');
  return said ? said.charAt(0).toUpperCase() + said.slice(1) : 'Each puts the plan’s limits back to empty';
}

/** A word of caution when the account's limits are far from full, so a reset now would free
 *  little; null when one is close to its limit. */
export function earlyNote(meters: Meter[]): string | null {
  if (meters.length === 0) return null;
  const fullest = Math.max(...meters.map((m) => m.percent));
  return fullest < 50 ? `Its fullest limit is at ${Math.round(fullest)}%, so a reset now would free little.` : null;
}
