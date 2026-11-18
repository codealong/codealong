// A session's conversation for the window's find (ui/WindowFind.tsx, ui/ThreadFind.tsx): how
// often the pages read but not laid out yet say what's found. What's laid out is searched as it
// reads on the page; a page further back is counted from its items, as near as plain text gets to
// how they show, so the count covers the whole conversation and the way back knows which page to
// bring in.

import { plainDirectives } from './directives';
import { BOLD, BOLD_UNDER, ITALIC } from './emphasis';
import { fileInLine } from './filePaths';
import type { TranscriptItem } from './types';
import { countIn, findPattern, says } from './windowFind';

/** What the window's find looks for in a session a search found by what was said in it, from
 *  `line`, the line it was found by, and the words typed (quick.ts `queryWords`): the words as
 *  typed when the line has them in that order, else the longest of them the line has, else the
 *  longest. The search finds a session with its words anywhere in what was said, but the find
 *  matches them only together. */
export function findFor(line: string, words: string[]): string {
  if (words.length === 0) return '';
  const phrase = words.join(' ');
  const pattern = findPattern(phrase);
  if (pattern && says(pattern, line)) return phrase;
  const longest = [...words].sort((a, b) => b.length - a.length);
  const lower = line.toLowerCase();
  return longest.find((w) => lower.includes(w)) ?? longest[0];
}

/** Code, then bold and italics, as Markdown.tsx reads them: what each holds is its one group. */
const MARKS = new RegExp(String.raw`\`([^\`]+)\`|${BOLD}|${BOLD_UNDER}|${ITALIC}`, 'g');

/** Text without the marks for code, bold and italics, read inside one another as Markdown.tsx
 *  `inline` reads them, code kept as it's written. */
const unmarked = (text: string): string => text.replace(MARKS, (_, code?: string, bold?: string, under?: string, italic?: string) => code ?? unmarked(bold ?? under ?? italic ?? ''));

/** Markdown as it reads once laid out, near enough: images gone, links as their words, and the
 *  marks for code, emphasis and headings left out where they're drawn, so a glob written without
 *  them (`apps/*`, `__tests__`) is found as it shows. */
export function plainOf(markdown: string): string {
  return unmarked(
    markdown
      .replace(/!\[[^\]]*\]\([^)]*\)/g, ' ')
      .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
      .replace(/^ {0,3}(#{1,6}|>+)\s+/gm, ''),
  );
}

/** What an item shows, as plain text: your prompt (a command you ran with `!` as you typed it), a
 *  reply, another session's message, a note, a tool call's line. A call's output shows only once it's opened, so it isn't
 *  counted, nor is a result, which shows under its call, but for your answers to the questions a
 *  call asked, which show as a message of yours (`answersShown`). */
export function shownText(it: TranscriptItem): string {
  switch (it.kind) {
    // A reply's Codex directives are drawn (Markdown.tsx), as another session's message's and a
    // prompt's are; Codealong's aren't.
    case 'you':
      return it.input != null ? `! ${it.input}` : plainOf(plainDirectives(it.text));
    case 'agent':
    case 'peer':
      return plainOf(plainDirectives(it.text));
    case 'codealong':
      return plainOf(it.text);
    // Files a call sent you show under its line, by what it said of them and their names.
    case 'tool':
      return [lineOf(it), it.sent?.caption, ...(it.sent?.files ?? []).map((f) => f.slice(f.lastIndexOf('/') + 1) || f)].filter(Boolean).join('\n');
    case 'note':
      return it.text;
    case 'result':
      return answersShown(it).join('\n');
    default:
      return '';
  }
}

/** A call's line as it shows, without a code span's marks, naming the file the call worked on
 *  as it's called, where its label doesn't name it (filePaths.ts `fileInLine`). */
function lineOf(it: TranscriptItem): string {
  const plain = (s: string) => s.replace(/`/g, '');
  if (!it.file) return plain(it.text);
  const { before, name, after } = fileInLine(it.text, it.file);
  return plain(before) + name + plain(after);
}

/** Your answers to a call's questions as they show (ui/SessionThread.tsx `YourAnswers`): each
 *  what you picked or wrote, as `answeredOf` (ui/QuestionList.tsx) puts it, with a code span's
 *  marks left out as `Rich` draws it and nothing else. */
function answersShown(it: TranscriptItem): string[] {
  return Object.values(it.answers ?? {})
    .filter((a) => a.length)
    .map((a) => a.join(', ').replace(/`/g, ''));
}

/** How many times a page's items say it. Your answers to questions are each laid out apart, so a
 *  match never runs from one into the next. */
export function pageHits(pattern: RegExp, items: TranscriptItem[]): number {
  return items.reduce((n, it) => n + (it.kind === 'result' ? answersShown(it) : [shownText(it)]).reduce((m, text) => m + countIn(pattern, text), 0), 0);
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
