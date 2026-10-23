// Paths a conversation writes in backticks, which open on a click (Markdown.tsx `PathCode`). Only
// a full path, or one under home (`~/…`), can be one, and only once the engine has said something
// is there (launch.rs `path_kinds`): a route (`/api/v1/users`), a command or a scratch file
// cleared since stays as it's written. A window asks once for each path it shows, the paths that
// come up together in one ask, and keeps what it heard; a path that wasn't there is asked again
// when it next shows, a while later, since an agent often names a file before writing it.

import { useEffect, useReducer } from 'react';
import { pathKinds } from './api';
import type { PathKind } from './types';

/** A path a piece of code names, and the line it points at (`file.rs:12`, `file.rs:12:4`). */
export interface PathAt {
  path: string;
  line: number | null;
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
