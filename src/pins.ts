// Pinned sessions and pinned worktrees: two lists the engine keeps in state.json, each in the
// order you put it. New pins go to the top.

import { useCallback, useEffect, useMemo, useState } from 'react';

export function sameList(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((x, i) => x === b[i]);
}

/**
 * Moves `key` to insertion point `index` among the pinned keys showing, and returns the full
 * pinned list. Pins hidden by the project or the filter keep their places.
 */
export function reorderPinned(all: string[], shown: string[], key: string, index: number): string[] {
  const from = shown.indexOf(key);
  if (from < 0) return all;
  const next = shown.filter((k) => k !== key);
  next.splice(Math.max(0, Math.min(next.length, index > from ? index - 1 : index)), 0, key);
  if (next.every((k, i) => k === shown[i])) return all;
  const slots = new Set(shown);
  let i = 0;
  return all.map((k) => (slots.has(k) ? next[i++] : k));
}

/**
 * Puts `key` at insertion point `index` among the pinned keys showing, and returns the full
 * pinned list: a pin moves there, and a key that wasn't pinned is pinned there. With no pins
 * showing, a new one goes to the top.
 */
export function placePinned(all: string[], shown: string[], key: string, index: number): string[] {
  if (all.includes(key)) return reorderPinned(all, shown, key, index);
  const at = Math.max(0, Math.min(shown.length, index));
  // Before the pin showing at that point, or after the last one showing.
  const i = at < shown.length ? all.indexOf(shown[at]) : shown.length > 0 ? all.indexOf(shown[shown.length - 1]) + 1 : 0;
  return [...all.slice(0, i), key, ...all.slice(i)];
}

export interface Pins {
  /** In your order. */
  keys: string[];
  set: Set<string>;
  /** Pins `key` at the top, or unpins it. */
  toggle: (key: string) => void;
  /** Moves `key` to insertion point `index` among `shown`, the pinned keys showing. */
  move: (shown: string[], key: string, index: number) => void;
  /** Puts `key` at insertion point `index` among `shown`, pinning it if it isn't. */
  place: (shown: string[], key: string, index: number) => void;
  /** Unpins `key`. */
  remove: (key: string) => void;
}

/**
 * One pinned list. `saved` is the engine's copy; a change shows at once and stays until the
 * engine's board catches up. If saving fails, the list goes back and `onError` hears why.
 */
export function usePins(saved: string[] | undefined, save: (keys: string[]) => Promise<void>, onError: (message: string) => void): Pins {
  const [override, setOverride] = useState<string[] | null>(null);
  const keys = useMemo(() => override ?? saved ?? [], [override, saved]);
  const set = useMemo(() => new Set(keys), [keys]);
  useEffect(() => {
    if (override && saved && sameList(saved, override)) setOverride(null);
  }, [saved, override]);
  const update = useCallback(
    (next: string[]) => {
      setOverride(next);
      save(next).catch((e: unknown) => {
        setOverride(null);
        onError(String(e));
      });
    },
    [save, onError],
  );
  const toggle = useCallback((key: string) => update(set.has(key) ? keys.filter((k) => k !== key) : [key, ...keys]), [keys, set, update]);
  const move = useCallback(
    (shown: string[], key: string, index: number) => {
      const next = reorderPinned(keys, shown, key, index);
      if (next !== keys) update(next);
    },
    [keys, update],
  );
  const place = useCallback(
    (shown: string[], key: string, index: number) => {
      const next = placePinned(keys, shown, key, index);
      if (next !== keys) update(next);
    },
    [keys, update],
  );
  const remove = useCallback(
    (key: string) => {
      if (set.has(key)) update(keys.filter((k) => k !== key));
    },
    [keys, set, update],
  );
  return useMemo(() => ({ keys, set, toggle, move, place, remove }), [keys, set, toggle, move, place, remove]);
}
