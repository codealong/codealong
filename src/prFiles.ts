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

/** A line of a file's diff as its view draws it: a hunk's header, a line added, removed or the
 *  same on both sides, or git's note that a side has no newline at its end. `old` and `new` are
 *  its numbers in the file before and after, where it's on that side. `text` is without its
 *  sign; a header keeps its own. */
export interface DiffLine {
  kind: 'hunk' | 'add' | 'del' | 'ctx' | 'note';
  text: string;
  old: number | null;
  new: number | null;
}

const HUNK = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/;

/** GitHub's patch for a file (its hunks, without the file's own header) as numbered lines. */
export function parsePatch(patch: string): DiffLine[] {
  const out: DiffLine[] = [];
  let old = 0;
  let neu = 0;
  const lines = patch.split('\n');
  // A patch that ends with a newline has nothing after it.
  if (lines.length > 1 && lines[lines.length - 1] === '') lines.pop();
  for (const l of lines) {
    const hunk = HUNK.exec(l);
    if (hunk) {
      old = Number(hunk[1]);
      neu = Number(hunk[2]);
      out.push({ kind: 'hunk', text: l, old: null, new: null });
    } else if (l.startsWith('+')) {
      out.push({ kind: 'add', text: l.slice(1), old: null, new: neu++ });
    } else if (l.startsWith('-')) {
      out.push({ kind: 'del', text: l.slice(1), old: old++, new: null });
    } else if (l.startsWith('\\')) {
      out.push({ kind: 'note', text: l.slice(1).trim(), old: null, new: null });
    } else {
      out.push({ kind: 'ctx', text: l.slice(1), old: old++, new: neu++ });
    }
  }
  return out;
}

/** A row of a diff drawn side by side: a header or note across both, or the file before on the
 *  left and after on the right, either missing where that side has no line there. */
export type SplitRow = { kind: 'hunk' | 'note'; line: DiffLine } | { kind: 'pair'; left: DiffLine | null; right: DiffLine | null };

/** Numbered lines (`parsePatch`) as rows side by side: a line the same on both sides on both, and
 *  each run of removed lines beside the run of added ones after it, line by line. */
export function splitRows(lines: DiffLine[]): SplitRow[] {
  const rows: SplitRow[] = [];
  let dels: DiffLine[] = [];
  let adds: DiffLine[] = [];
  const flush = () => {
    for (let i = 0; i < Math.max(dels.length, adds.length); i++) rows.push({ kind: 'pair', left: dels[i] ?? null, right: adds[i] ?? null });
    dels = [];
    adds = [];
  };
  for (const l of lines) {
    if (l.kind === 'del') {
      // Removed lines after added ones start a run of their own.
      if (adds.length > 0) flush();
      dels.push(l);
    } else if (l.kind === 'add') {
      adds.push(l);
    } else {
      flush();
      rows.push(l.kind === 'ctx' ? { kind: 'pair', left: l, right: l } : { kind: l.kind, line: l });
    }
  }
  flush();
  return rows;
}

/** A changed file's place on its PR's Files changed page on GitHub, `<pr url>/files#diff-<the
 *  SHA-256 of its path, in hex>`, as GitHub names it there. */
export async function fileOnGitHub(prUrl: string, path: string): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(path));
  const hex = Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, '0')).join('');
  return `${prUrl}/files#diff-${hex}`;
}

/** The line a file opens at for its changes: its first added line, else where its first hunk
 *  starts on the side after. */
export function firstChanged(lines: DiffLine[]): number | null {
  const added = lines.find((l) => l.kind === 'add');
  if (added) return added.new;
  return lines.find((l) => l.new !== null)?.new ?? null;
}
