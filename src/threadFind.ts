// Finding words in a session's conversation (⌘F in its panel, ui/ThreadFind.tsx): what a query
// matches, and how often the pages read but not laid out yet say it. What's laid out is searched
// as it reads on the page; a page further back is counted from its items, as near as plain text
// gets to how they show, so the count covers the whole conversation and the way back knows which
// page to bring in.

import type { TranscriptItem } from './types';

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

/** Markdown as it reads once laid out, near enough: images gone, links as their words, and the
 *  marks for emphasis, code and headings left out. */
export function plainOf(markdown: string): string {
  return markdown
    .replace(/!\[[^\]]*\]\([^)]*\)/g, ' ')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/^ {0,3}(#{1,6}|>+)\s+/gm, '')
    .replace(/\*\*|__|~~|[*`]/g, '');
}

/** What an item shows, as plain text: your prompt (a command you ran with `!` as you typed it), a
 *  reply, a note, a tool call's line. A call's output shows only once it's opened, so it isn't
 *  counted, nor is a result, which shows under its call. */
export function shownText(it: TranscriptItem): string {
  switch (it.kind) {
    case 'you':
      return it.input != null ? `! ${it.input}` : plainOf(it.text);
    case 'agent':
      return plainOf(it.text);
    case 'tool':
      return it.text.replace(/`/g, '');
    case 'note':
      return it.text;
    default:
      return '';
  }
}

/** How many times a page's items say it. */
export function pageHits(pattern: RegExp, items: TranscriptItem[]): number {
  return items.reduce((n, it) => n + countIn(pattern, shownText(it)), 0);
}

/** Pages read back from the conversation's start in view, as a chain: from `before` (where what's
 *  shown begins), each page read at a cursor leads to the one at its own `before`. The ones read
 *  so far, nearest first, and whether the chain reaches the session's start. */
export function chainFrom<P extends { before: number | null }>(before: number | null, read: ReadonlyMap<number, P>): { pages: { at: number; page: P }[]; whole: boolean } {
  const pages: { at: number; page: P }[] = [];
  let at = before;
  while (at != null) {
    const page = read.get(at);
    if (!page) return { pages, whole: false };
    pages.push({ at, page });
    at = page.before;
  }
  return { pages, whole: true };
}

/** What the find's count says: where the current match is among them all, counted from the top,
 *  and how many there are. `partial` while pages further back haven't been read (they're being
 *  read, unless one couldn't be), and `looking` while it goes back to the match before. */
export function countWords({ at, total, partial, reading, looking }: { at: number | null; total: number; partial: boolean; reading: boolean; looking: boolean }): string {
  if (looking) return 'Looking earlier…';
  const sofar = partial ? ' so far' : '';
  if (total === 0) return reading ? 'Looking…' : `No matches${sofar}`;
  return at == null ? `${total}${sofar}` : `${at + 1} of ${total}${sofar}`;
}
