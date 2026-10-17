// What a conversation says at its end while its agent works (ui/Working.tsx): a word for what it's
// up to, for fun rather than news, which changes every so often, and how long the turn has gone
// on. Each session has the words in an order of its own, and which one shows goes by the clock, so
// every window showing a session says the same word, and sessions working side by side say
// different ones. A turn that has gone on a long while gets a word for that now and then.
//
// Now and then, a long wait gets a tip on using Codealong under the line (tips.ts, the `working`
// place), quiet and only one at a time: none in the first moments of a turn, which are often all
// of it, and none for a while after the last, wherever it showed. A turn keeps the tip it got,
// in every window, until it ends, though other turns get theirs meanwhile.

/** What it might be up to; a word shows with an ellipsis after it. Sentence case, no stop. */
export const WORKING_WORDS: readonly string[] = [
  'Tinkering',
  'Noodling',
  'Puttering',
  'Pondering',
  'Mulling it over',
  'Rummaging around',
  'Whittling',
  'Untangling',
  'Sketching it out',
  'Connecting the dots',
  'Chipping away',
  'Humming along',
  'Turning it over',
  'Poking around',
  'Piecing it together',
  'Sorting it out',
  'Brewing',
  'Cooking something up',
  'Following a hunch',
  'Tidying up',
  'Measuring twice',
  'Reading the fine print',
  'Lining things up',
  'Sharpening pencils',
  'Rolling up its sleeves',
  'Thinking it through',
  'Doodling',
  'Hatching a plan',
  'Simmering',
  'Tracing the threads',
  'Squinting at it',
  'Scribbling',
  'Weighing it up',
  'Nudging things along',
  'Counting semicolons',
  'Untying knots',
  'Taking notes',
  'Getting its bearings',
  'Kneading',
  'Fiddling',
];

/** For a turn that has gone on a long while, every third word. */
export const LONG_WORDS: readonly string[] = [
  'Still at it',
  'In it for the long haul',
  'Going the distance',
  'Deep in the weeds',
  'Making a day of it',
  'Settling in',
  'Steady on',
  'Keeping at it',
];

/** How long a word shows before the next. */
export const WORD_FOR_MS = 30_000;
/** A turn this long gets a word for that, every third. */
export const LONG_MS = 15 * 60_000;
/** A turn gets a tip only once it has worked this long. */
export const TIP_AFTER_MS = 20_000;
/** And only this long after the last tip a turn got. */
export const TIP_EVERY_MS = 20 * 60_000;
/** How long the tip a turn got is kept, for the turn while it goes on: longer than any turn, and
 *  at a tip every `TIP_EVERY_MS` at most, a few dozen claims. */
export const CLAIM_FOR_MS = 24 * 60 * 60_000;

/** A number from text, the same every time (FNV-1a). */
function hashOf(text: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 0x01000193);
  return h >>> 0;
}

/** `words` in an order of `seed`'s own, the same every time. */
export function shuffled(words: readonly string[], seed: string): string[] {
  const out = [...words];
  let state = hashOf(seed);
  // mulberry32: enough to shuffle a list with.
  const next = () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  for (let i = out.length - 1; i > 0; i--) {
    const j = Math.floor(next() * (i + 1));
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out;
}

/** The word session `key` says at `now`, working since `since`: its own order of the words, one
 *  every `WORD_FOR_MS` by the clock, so no two in a row are the same. */
export function workingWord(key: string, since: number, now: number): string {
  const tick = Math.floor(now / WORD_FOR_MS);
  if (now - since >= LONG_MS && tick % 3 === 2) {
    const long = shuffled(LONG_WORDS, `${key}:long`);
    return long[Math.floor(tick / 3) % long.length];
  }
  const words = shuffled(WORKING_WORDS, key);
  return words[tick % words.length];
}

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
