// A PR's changed files as the filter over Files changed narrows them (PrFiles.tsx). What's typed
// is words: a file is listed when its path holds every one of them, wherever and in any case, so
// `select team` finds `entity-select/TeamMultiSelect.tsx`; a word after `-` leaves out the files
// whose path holds it (`-spec`). Files whose paths don't match but whose changes mention every
// word come after, as the ones that mention it, so a name can be looked for in what the PR
// changed as well as in where. Where the words fall is marked, in the paths and in the diffs.

import type { PrFile } from './types';

/** What's typed, as words to find and words that leave a file out, in lower case. */
export interface FileQuery {
  want: string[];
  skip: string[];
}

export function fileQuery(text: string): FileQuery {
  const want: string[] = [];
  const skip: string[] = [];
  for (const word of text.toLowerCase().split(/\s+/)) {
    if (!word) continue;
    // A `-` alone is a word like any other: a path with a dash in it.
    if (word.length > 1 && word.startsWith('-')) skip.push(word.slice(1));
    else want.push(word);
  }
  return { want, skip };
}

/** Where `words` fall in `text`, in any case: start and end of each stretch, in order, the ones
 *  that touch or overlap made one. */
export function marks(text: string, words: string[]): [number, number][] {
  const lower = text.toLowerCase();
  const found: [number, number][] = [];
  for (const w of words) {
    if (!w) continue;
    for (let at = lower.indexOf(w); at >= 0; at = lower.indexOf(w, at + w.length)) found.push([at, at + w.length]);
  }
  found.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  const merged: [number, number][] = [];
  for (const [from, to] of found) {
    const last = merged[merged.length - 1];
    if (last && from <= last[1]) last[1] = Math.max(last[1], to);
    else merged.push([from, to]);
  }
  return merged;
}

/** `text` from `from` to `to` in pieces, each marked or not as `ranges` (from `marks`) has it. */
export function pieces(text: string, ranges: [number, number][], from = 0, to = text.length): { text: string; hit: boolean }[] {
  const out: { text: string; hit: boolean }[] = [];
  let at = from;
  for (const [a, b] of ranges) {
    const start = Math.max(a, from);
    const end = Math.min(b, to);
    if (end <= start) continue;
    if (start > at) out.push({ text: text.slice(at, start), hit: false });
    out.push({ text: text.slice(start, end), hit: true });
    at = end;
  }
  if (at < to) out.push({ text: text.slice(at, to), hit: false });
  return out;
}

/** A path as its rows say it: the file's name, then the folder it's in. */
export function splitPath(path: string): { dir: string; name: string } {
  const cut = path.lastIndexOf('/');
  return cut < 0 ? { dir: '', name: path } : { dir: path.slice(0, cut), name: path.slice(cut + 1) };
}

/** A file the filter lists, with where the words fall in its path, and, for one listed for its
 *  changes, how many lines of them mention one. */
export interface FileHit {
  file: PrFile;
  inPath: [number, number][];
  lines: number;
}

/** The files the filter lists: `named`, whose paths hold every word, in GitHub's order, then
 *  `mentioned`, whose changes do and paths don't. With no words to find, every file not left out
 *  is named. */
export function filterFiles(files: PrFile[], text: string): { named: FileHit[]; mentioned: FileHit[] } {
  const { want, skip } = fileQuery(text);
  const named: FileHit[] = [];
  const mentioned: FileHit[] = [];
  for (const file of files) {
    const path = file.path.toLowerCase();
    if (skip.some((w) => path.includes(w))) continue;
    if (want.every((w) => path.includes(w))) {
      named.push({ file, inPath: marks(file.path, want), lines: 0 });
      continue;
    }
    const patch = file.patch?.toLowerCase();
    if (!patch || !want.every((w) => patch.includes(w))) continue;
    const lines = patch.split('\n').filter((l) => want.some((w) => l.includes(w))).length;
    mentioned.push({ file, inPath: [], lines });
  }
  return { named, mentioned };
}
