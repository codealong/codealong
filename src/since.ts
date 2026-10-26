// What changed on a PR since the review you last gave it (`PrItem.lastReview`), for the pill
// beside the line over its box (PrBox.tsx) and its Changes tab (PrFiles.tsx): GitHub's comparison
// of the commit your review was of with the one the PR's at (github.rs `since`, REST, so no
// GraphQL points), read once for each pair while something shows it, for every part of the window,
// and kept: two commits' comparison doesn't change, so a reload (reload.ts) reads again only one
// that failed. Whether a PR's Changes tab shows only those changes is kept for each PR while the
// window goes from one to the next; the pill turns it on as it shows the tab.

import { useEffect, useSyncExternalStore } from 'react';
import { prSince } from './api';
import { onReload, useReloads } from './reload';
import { errorText } from './report';
import type { Since } from './types';

/** What's known of a comparison: its answer, or why GitHub gave none. */
export interface SinceHeld {
  since?: Since;
  error?: string;
  loading: boolean;
}

const held = new Map<string, SinceHeld>();
const shown = new Map<string, boolean>();
const listeners = new Set<() => void>();
const tell = () => listeners.forEach((l) => l());
const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => void listeners.delete(l);
};

const keyOf = (repo: string, from: string, to: string) => `${repo}@${from}...${to}`;

onReload(() => {
  for (const [key, h] of held) if (h.error) held.delete(key);
});

function read(repo: string, from: string, to: string) {
  const key = keyOf(repo, from, to);
  held.set(key, { loading: true });
  tell();
  prSince(repo, from, to).then(
    (since) => {
      held.set(key, { since, loading: false });
      tell();
    },
    (e: unknown) => {
      held.set(key, { error: errorText(e), loading: false });
      tell();
    },
  );
}

/** What changed on `repo`'s branch from commit `from` to `to`, read as it's first wanted; none
 *  without both. */
export function useSince(repo: string, from: string | null, to: string | null): SinceHeld | undefined {
  const key = from && to ? keyOf(repo, from, to) : null;
  const reloads = useReloads();
  useEffect(() => {
    if (!key || !from || !to || held.has(key)) return;
    read(repo, from, to);
  }, [key, repo, from, to, reloads]);
  return useSyncExternalStore(subscribe, () => (key ? held.get(key) : undefined));
}

/** Reads a comparison again, after GitHub gave none. */
export function readSinceAgain(repo: string, from: string, to: string) {
  read(repo, from, to);
}

/** Whether PR `key`'s Changes tab shows only what changed since your review. */
export function useSinceShown(key: string): boolean {
  return useSyncExternalStore(subscribe, () => shown.get(key) ?? false);
}

export function showSince(key: string, on: boolean) {
  shown.set(key, on);
  tell();
}
