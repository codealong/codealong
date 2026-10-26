// ⌘R reloads what a window shows, as a browser's reload does, but keeps your place in it: the
// engine asks GitHub about the PRs again now (`refresh`, unless it just did), and the board's
// windows, which show plan usage, have it read usage again as the usage bar's button does. What
// the window read for itself of what it shows is read again: a PR's details, how it merges and
// your review of it on GitHub, the agent's reviews of it, and what failed to read (what changed
// since your review, a conversation). The page itself isn't reloaded: the windows that float are
// told what to show only as it changes, and a reloaded one would forget it.
//
// Each of those reads goes by the window's count of reloads (`useReloads`) as well as by what
// it's of, and the caches that would answer it instead forget first (`onReload`). What was read
// stays up until the new read takes its place, or beside the error when that fails too.

import { useSyncExternalStore } from 'react';
import { refresh, refreshUsage } from './api';
import { chordPressed } from './platform';

/** ⌘R; Ctrl+R on Linux (platform.ts). */
export const RELOAD_CHORD = 'Cmd+R';

/** Whether a key asks to reload. */
export function reloadPressed(e: Pick<KeyboardEvent, 'key' | 'code' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>): boolean {
  return chordPressed(RELOAD_CHORD, e);
}

let count = 0;
const listeners = new Set<() => void>();
const forgets = new Set<() => void>();

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => void listeners.delete(l);
};

/** Has a cache of what the window read forget it as the window reloads, before anything reads
 *  again. */
export function onReload(forget: () => void): () => void {
  forgets.add(forget);
  return () => void forgets.delete(forget);
}

/** Reloads what this window shows; `usage` for a window that shows plan usage. */
export function reloadWindow({ usage }: { usage: boolean }): void {
  void refresh();
  if (usage) void refreshUsage();
  for (const forget of forgets) forget();
  count += 1;
  for (const l of listeners) l();
}

/** How many times this window has reloaded: a read that goes by it is made again at each. */
export function useReloads(): number {
  return useSyncExternalStore(subscribe, () => count);
}
