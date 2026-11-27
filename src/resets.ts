// The limit resets an account holds, as the engine reads them (resets.rs): which accounts the
// status bar marks (only once close to a limit), and the words its card uses.
import { count, duration } from './format';
import type { Meter, Resets, Usage } from './types';

const DAY = 24 * 60 * 60 * 1000;

/** How full a limit is before an account is close enough to its limits to show its reset option:
 *  where a meter turns to warning (usage.rs `severity`). */
export const CLOSE_TO_LIMIT = 75;

/** An account's reading, when it's been read. */
export function resetsOf(list: Resets[] | undefined, account: string): Resets | undefined {
  return list?.find((r) => r.account === account);
}

/** The account holds at least one reset. */
export function holds(r: Resets | undefined): r is Resets {
  return !!r && r.available > 0;
}

/** Whether any of the account's limits is close to running out (at warning or critical, or at
 *  least CLOSE_TO_LIMIT% used). Unread accounts have no meters and aren't close. */
export function closeToLimit(meters: Meter[]): boolean {
  return meters.some((m) => m.severity !== 'normal' || m.percent >= CLOSE_TO_LIMIT);
}

/** The resets to show on `u`'s chip and card: only when it's signed in, holds some, and is close
 *  to a limit. */
export function offered(list: Resets[] | undefined, u: Usage): Resets | undefined {
  if (u.needsSignIn || !closeToLimit(u.meters)) return undefined;
  const r = resetsOf(list, u.account);
  return holds(r) ? r : undefined;
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
