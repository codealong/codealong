// When the line at a conversation's end while its agent works (ui/Working.tsx) gets a tip on
// using Codealong (tips.ts, the `working` place): quiet and only one at a time, none in the first
// moments of a turn, which are often all of it, and none for a while after the last, wherever it
// showed. A turn keeps the tip it got, in every window, until it ends, though other turns get
// theirs meanwhile.

/** A turn gets a tip only once it has worked this long. */
export const TIP_AFTER_MS = 20_000;
/** And only this long after the last tip a turn got. */
export const TIP_EVERY_MS = 20 * 60_000;
/** How long the tip a turn got is kept, for the turn while it goes on: longer than any turn, and
 *  at a tip every `TIP_EVERY_MS` at most, a few dozen claims. */
export const CLAIM_FOR_MS = 24 * 60 * 60_000;

/** A tip a turn got: the turn (its session and when its prompt was sent), the tip and when. */
export interface TipClaim {
  turn: string;
  id: string;
  at: number;
}

/** A turn, as a tip's claim names it: a session's turn holds its prompt's time while it asks you
 *  things and goes on. */
export const turnOf = (key: string, promptAt: number) => `${key}:${promptAt}`;

/** Whether a turn, worked `worked` so far, shows a tip: the one it got (`keep`), a new one
 *  (`take`), or none. `claims` are the tips turns got, the latest last. */
export function workingTip(claims: readonly TipClaim[], turn: string, worked: number, now: number): { keep: TipClaim } | 'take' | null {
  const got = claims.find((c) => c.turn === turn);
  if (got) return { keep: got };
  if (worked < TIP_AFTER_MS) return null;
  const last = claims[claims.length - 1];
  return !last || now - last.at >= TIP_EVERY_MS || last.at > now ? 'take' : null;
}

/** `claims` with `claim` the latest, less those older than `CLAIM_FOR_MS`. */
export function claimed(claims: readonly TipClaim[], claim: TipClaim): TipClaim[] {
  return [...claims.filter((c) => c.turn !== claim.turn && claim.at - c.at < CLAIM_FOR_MS && c.at <= claim.at), claim];
}
