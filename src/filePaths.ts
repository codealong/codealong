// Paths a conversation writes in backticks, which open on a click (Markdown.tsx `PathCode`). Only
// a full path, or one under home (`~/…`), can be one, and only once the engine has said something
// is there (launch.rs `path_kinds`): a route (`/api/v1/users`), a command or a scratch file
// cleared since stays as it's written. A window asks once for each path it shows, the paths that
// come up together in one ask, and keeps what it heard; a path that wasn't there is asked again
// when it next shows, a while later, since an agent often names a file before writing it.

import { useEffect, useReducer } from 'react';
import { pathKinds, sentFiles } from './api';
import { mediaKind } from './sentFiles';
import type { PathKind, SentState } from './types';

/** A path a piece of code names, and the line it points at (`file.rs:12`, `file.rs:12:4`). */
export interface PathAt {
  path: string;
  line: number | null;
}

/** Whether a path names a Markdown file Codealong can preview in place (`read_markdown`). */
export function isMarkdownFile(path: string): boolean {
  return /\.(?:md|markdown)$/i.test(path);
}

/** Parses a local `file://` URL (with optional `localhost`, percent-encoding, `#L12` / `#L12-L20`
 *  or `:12` line suffix) into a local path and line. Remote `file://host/…` URLs return null. */
export function fileUrlAt(url: string): PathAt | null {
  if (!/^file:\/\//i.test(url) || url.length > 2048 || url.includes('\n')) return null;
  let rest = url.slice('file://'.length);
  if (/^localhost\//i.test(rest)) rest = rest.slice('localhost'.length);
  if (!rest.startsWith('/')) return null;

  let line: number | null = null;
  const hash = rest.indexOf('#');
  if (hash >= 0) {
    const frag = rest.slice(hash + 1);
    rest = rest.slice(0, hash);
    const lm = /^L?(\d+)(?:-L?\d+)?$/i.exec(frag);
    if (lm) line = Number(lm[1]);
  }
  const q = rest.indexOf('?');
  if (q >= 0) rest = rest.slice(0, q);

  let path: string;
  try {
    path = decodeURIComponent(rest);
  } catch {
    return null;
  }
  // Windows `file:///C:/Users/...` -> `C:/Users/...`.
  if (/^\/[A-Za-z]:[\\/]/.test(path)) path = path.slice(1);
  if (line == null) {
    const colon = /^(.*?):(\d+)(?::\d+)?$/.exec(path);
    if (colon && colon[1]) {
      path = colon[1];
      line = Number(colon[2]);
    }
  }
  if (!path || path === '/' || path.length > 1024 || path.includes('\n')) return null;
  return { path, line };
}

/** Local path for an inline Markdown image (`![alt](/abs/path.png)` or `![alt](file:///…)`), when
 *  its extension is an image `sentfiles.rs` serves; null for web URLs, relative paths and non-images. */
export function localImagePath(target: string): string | null {
  const trimmed = target.trim().replace(/^<(.+)>$/, '$1').replace(/\s+(?:"[^"]*"|'[^']*')$/, '');
  let path: string;
  if (/^file:\/\//i.test(trimmed)) {
    const parsed = fileUrlAt(trimmed);
    if (!parsed) return null;
    path = parsed.path;
  } else if ((trimmed.startsWith('/') && !trimmed.startsWith('//')) || /^[A-Za-z]:[\\/]/.test(trimmed)) {
    path = trimmed;
  } else {
    return null;
  }
  return mediaKind(path) === 'image' ? path : null;
}

/** The path a piece of code may be: full or under home, of two parts at least (`/release` is a
 *  skill), with no protocol's `//`. A line after it is kept apart. Anything else a name can hold
 *  is let through (`Plan - final.md`, `R&D`, `Client's notes.md`): whether it's a file is the
 *  engine's to say, and a command written as code (`/usr/bin/open -R x`) is nothing it finds. */
export function pathIn(code: string): PathAt | null {
  if (code.length > 1024 || code.includes('\n') || code.includes('//') || code !== code.trim()) return null;
  const m = /^(.*?)(?::(\d+)(?::\d+)?)?$/.exec(code);
  const path = m?.[1] ?? '';
  const parts = path.startsWith('~/') ? path.slice(2).split('/') : path.startsWith('/') ? path.slice(1).split('/') : [];
  const named = parts.filter(Boolean);
  if (named.length < (path.startsWith('~/') ? 1 : 2) || named.length < parts.length - 1) return null;
  return { path, line: m?.[2] ? Number(m[2]) : null };
}

/** What a path is, as this window last heard, and when. */
const known = new Map<string, { kind: PathKind | null; at: number }>();
/** Who waits to hear about a path. */
const waiting = new Map<string, Set<() => void>>();
/** The paths to ask about next, gathered while the page draws. */
let asking: Set<string> | null = null;

/** How long a path that wasn't there is believed, before showing it again asks again. */
const AGAIN_MS = 30_000;

function flush() {
  const paths = [...(asking ?? [])];
  asking = null;
  const heard = (kinds: (PathKind | null)[]) => {
    const at = Date.now();
    paths.forEach((p, i) => known.set(p, { kind: kinds[i] ?? null, at }));
    paths.forEach((p) => waiting.get(p)?.forEach((cb) => cb()));
  };
  // Not heard is not there: it's asked again as it shows again, a while later.
  pathKinds(paths).then(heard, () => heard([]));
}

function ask(path: string) {
  if (!asking) {
    asking = new Set();
    setTimeout(flush, 0);
  }
  asking.add(path);
}

/** What `path` is on this Mac: a file, a folder, or null while it's being asked and where nothing
 *  is. Null asks nothing. */
export function usePathKind(path: string | null): PathKind | null {
  const [, heard] = useReducer((n: number) => n + 1, 0);
  useEffect(() => {
    if (!path) return;
    let cbs = waiting.get(path);
    if (!cbs) waiting.set(path, (cbs = new Set()));
    cbs.add(heard);
    // A path being asked about is marked as heard just now, as nothing, so another showing it
    // meanwhile waits for the answer rather than ask again.
    const was = known.get(path);
    if (!was || (was.kind === null && Date.now() - was.at > AGAIN_MS)) {
      known.set(path, { kind: null, at: Date.now() });
      ask(path);
    }
    return () => {
      cbs.delete(heard);
      if (cbs.size === 0) waiting.delete(path);
    };
  }, [path]);
  return path ? (known.get(path)?.kind ?? null) : null;
}

/** What a local media file is on this Mac (`sent_files`), batched like `usePathKind` so several
 *  inline images in one reply ask the engine once. */
const knownMedia = new Map<string, { state: SentState; at: number }>();
const waitingMedia = new Map<string, Set<() => void>>();
let askingMedia: Set<string> | null = null;

function flushMedia() {
  const paths = [...(askingMedia ?? [])];
  askingMedia = null;
  const heard = (states: SentState[]) => {
    const at = Date.now();
    paths.forEach((p, i) => knownMedia.set(p, { state: states[i] ?? 'gone', at }));
    paths.forEach((p) => waitingMedia.get(p)?.forEach((cb) => cb()));
  };
  sentFiles(paths).then(heard, () => heard(paths.map(() => 'here')));
}

function askMedia(path: string) {
  if (!askingMedia) {
    askingMedia = new Set();
    setTimeout(flushMedia, 0);
  }
  askingMedia.add(path);
}

/** Whether a local media file can be shown (`here`), needs a click first (`asksFirst`), or isn't
 *  there (`gone`); null while being asked or when `path` is null. */
export function useSentState(path: string | null): SentState | null {
  const [, heard] = useReducer((n: number) => n + 1, 0);
  useEffect(() => {
    if (!path) return;
    let cbs = waitingMedia.get(path);
    if (!cbs) waitingMedia.set(path, (cbs = new Set()));
    cbs.add(heard);
    const was = knownMedia.get(path);
    if (!was || (was.state === 'gone' && Date.now() - was.at > AGAIN_MS)) {
      askMedia(path);
    }
    return () => {
      cbs.delete(heard);
      if (cbs.size === 0) waitingMedia.delete(path);
    };
  }, [path]);
  return path ? (knownMedia.get(path)?.state ?? null) : null;
}

/** A call's line split where it names the file the call read or changed (`TranscriptItem.file`),
 *  by the file's name in backticks, for the name to open the file (SessionThread.tsx `ToolLine`).
 *  A line that doesn't name it (Antigravity's says what the call is for) has it after it. */
export function fileInLine(text: string, file: string): { before: string; name: string; after: string } {
  // A Windows path's name follows its last backslash, where a POSIX name can have one in it.
  const windows = /^([A-Za-z]:[\\/]|\\\\)/.test(file);
  const cut = windows ? Math.max(file.lastIndexOf('/'), file.lastIndexOf('\\')) : file.lastIndexOf('/');
  const name = file.slice(cut + 1) || file;
  // Found as the engine writes it in code (util.rs `code`), a backtick turned to a quote.
  const written = `\`${name.replace(/`/g, "'")}\``;
  const at = text.lastIndexOf(written);
  if (at < 0) return { before: `${text} · `, name, after: '' };
  return { before: text.slice(0, at), name, after: text.slice(at + written.length) };
}
