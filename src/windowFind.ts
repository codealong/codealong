// Finding words in a window, as a browser's find does (ui/WindowFind.tsx): what a query matches,
// where in a run of text each match falls, which match a new find starts on, where a step goes
// and what the count says. The walk over the window's text and the marks are WindowFind.tsx's;
// a conversation's pages further back are counted from their items (threadFind.ts).

/** The most matches a find marks and steps through, as browsers stop counting somewhere: past
 *  it the count says "1000+". A word as short as a letter would otherwise mark the window over. */
export const MOST = 1000;

/** What a query finds: its words in order, in any case, each run of spaces in it matching any run
 *  of whitespace, a line break included. Null when nothing's typed. */
export function findPattern(query: string): RegExp | null {
  const words = query.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return null;
  return new RegExp(words.map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('\\s+'), 'giu');
}

/** How many times `text` says it. */
export function countIn(pattern: RegExp, text: string): number {
  return text ? (text.match(pattern)?.length ?? 0) : 0;
}

/** Whether `text` says it at all. */
export function says(pattern: RegExp, text: string): boolean {
  return text.search(pattern) >= 0;
}

/** A match in a run of text read from pieces (the text nodes of a line, a word in bold among
 *  them): the piece it starts in and where in it, and the piece it ends in and where. */
export interface Span {
  first: number;
  from: number;
  last: number;
  to: number;
}

/** The matches in `text`, read from pieces that start at `starts`, as spans of the pieces, in
 *  order, as they're asked for: a find stops reading once it has as many as it takes. A match
 *  runs across pieces as the words read across them. */
export function* spansIn(text: string, starts: readonly number[], pattern: RegExp): Generator<Span> {
  if (starts.length === 0) return;
  let first = 0;
  for (const m of text.matchAll(pattern)) {
    const from = m.index;
    const to = from + m[0].length;
    while (first + 1 < starts.length && starts[first + 1] <= from) first++;
    let last = first;
    while (last + 1 < starts.length && starts[last + 1] < to) last++;
    yield { first, from: from - starts[first], last, to: to - starts[last] };
  }
}

/** Where a match is against the view it's in: above it, in it, or below it. */
export type Place = -1 | 0 | 1;

/** The match a new find starts on, of matches in the order they read, by where each is: the
 *  first in view; else the first below it, as a browser finds on from where you are; else the
 *  last above it, the nearest to where you're reading. -1 when there are none. */
export function startOf(places: readonly Place[]): number {
  const shown = places.indexOf(0);
  if (shown >= 0) return shown;
  const below = places.indexOf(1);
  if (below >= 0) return below;
  return places.length - 1;
}

/** Where a step goes from match `at` (null: none yet) among `count`: on to the next (1) or back
 *  to the one before (-1), round from the last to the first and back. -1 when there are none. */
export function stepTo(at: number | null, count: number, dir: -1 | 1): number {
  if (count === 0) return -1;
  if (at == null || at < 0) return dir > 0 ? 0 : count - 1;
  return (at + dir + count) % count;
}

/** What the find's count says: where the current match is among them all, counted from the top,
 *  and how many there are. `capped` when the window says it more than `MOST` times; `partial`
 *  while a conversation's pages further back haven't been read (they're being read, unless one
 *  couldn't be); `looking` while it goes back to the match before. */
export function countWords({ at, total, capped = false, partial, reading, looking }: { at: number | null; total: number; capped?: boolean; partial: boolean; reading: boolean; looking: boolean }): string {
  if (looking) return 'Looking earlier…';
  const sofar = partial ? ' so far' : '';
  if (total === 0) return reading ? 'Looking…' : `No matches${sofar}`;
  const of = `${total}${capped ? '+' : ''}`;
  return at == null ? `${of}${sofar}` : `${at + 1} of ${of}${sofar}`;
}
