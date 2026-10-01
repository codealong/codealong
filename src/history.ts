// The sessions you looked at in a window, in order, for going back to the one before and
// forward again (⌃- and ⌃⇧-, as in VS Code). It works as a browser's history does: looking at a
// session from somewhere back in the list drops what was ahead.

import { useCallback, useEffect, useRef, useState } from 'react';

export interface History {
  /** Session keys, oldest first. */
  keys: string[];
  /** The one you're on, or were on last; -1 with none. */
  at: number;
}

export const NO_HISTORY: History = { keys: [], at: -1 };
/** As many as a long day's worth; the oldest go first. */
const LIMIT = 100;
/** A session counts as looked at once its conversation has shown this long, so the ones passed
 *  over on the way down a list aren't stops on the way back. */
export const DWELL_MS = 1000;

/** `h` after looking at `key`: it follows the one you're on, in place of what was ahead. */
export function visit(h: History, key: string): History {
  if (h.keys[h.at] === key) return h;
  const keys = [...h.keys.slice(0, h.at + 1), key].slice(-LIMIT);
  return { keys, at: keys.length - 1 };
}

/**
 * Where Back goes from `shown`, the session showing now (null for none): the nearest one before
 * it that `ok` says can still be shown, or null with nowhere to go. A session showing that isn't
 * in the list yet is put there first, so Forward comes back to it; from no session at all, Back
 * is to the last one you were on. Sessions that can't be shown are passed over but kept: one
 * can come back (unarchived, say).
 */
export function back(h: History, shown: string | null, ok: (key: string) => boolean): History | null {
  const from = shown === null ? h : visit(h, shown);
  for (let i = shown === null ? from.at : from.at - 1; i >= 0; i--) {
    if (from.keys[i] !== shown && ok(from.keys[i])) return { keys: from.keys, at: i };
  }
  return null;
}

/** Where Forward goes: the nearest one after the one you're on. From a session that isn't the
 *  one the list is on there's nothing ahead: it's the newest. */
export function forward(h: History, shown: string | null, ok: (key: string) => boolean): History | null {
  if (shown !== null && shown !== h.keys[h.at]) return null;
  for (let i = h.at + 1; i < h.keys.length; i++) {
    if (h.keys[i] !== shown && ok(h.keys[i])) return { keys: h.keys, at: i };
  }
  return null;
}

/** What storage kept, or none if it isn't a history. */
export function parseHistory(text: string | null): History {
  try {
    const v: unknown = JSON.parse(text ?? '');
    if (!v || typeof v !== 'object') return NO_HISTORY;
    const { keys, at } = v as { keys?: unknown; at?: unknown };
    if (!Array.isArray(keys) || !keys.every((k) => typeof k === 'string') || keys.length === 0) return NO_HISTORY;
    const kept = (keys as string[]).slice(-LIMIT);
    const i = typeof at === 'number' && Number.isInteger(at) ? at - (keys.length - kept.length) : kept.length - 1;
    return { keys: kept, at: Math.min(kept.length - 1, Math.max(0, i)) };
  } catch {
    return NO_HISTORY;
  }
}

export interface SessionHistory {
  /** The session Back would show, or null with none. */
  back: string | null;
  forward: string | null;
  /** Goes there, and answers the session to show (null with nowhere to go). */
  goBack: () => string | null;
  goForward: () => string | null;
}

/**
 * A window's history of sessions. `shown` is the session whose conversation shows now, `ok`
 * whether a session can still be shown. It's kept under `storageKey`, so a relaunch doesn't lose
 * the way back.
 */
export function useSessionHistory(shown: string | null, ok: (key: string) => boolean, storageKey: string): SessionHistory {
  const [history, setHistory] = useState<History>(() => {
    try {
      return parseHistory(window.localStorage.getItem(storageKey));
    } catch {
      return NO_HISTORY;
    }
  });
  const keep = useCallback(
    (h: History) => {
      setHistory(h);
      try {
        window.localStorage.setItem(storageKey, JSON.stringify(h));
      } catch {
        // Storage can be unavailable; the history just won't outlast the window.
      }
    },
    [storageKey],
  );
  // The latest, for the timer and for a key pressed before the next render.
  const now = useRef(history);
  now.current = history;
  useEffect(() => {
    if (shown === null) return;
    const t = window.setTimeout(() => {
      const next = visit(now.current, shown);
      if (next !== now.current) keep(next);
    }, DWELL_MS);
    return () => window.clearTimeout(t);
  }, [shown, keep]);
  const go = (to: History | null) => {
    if (!to) return null;
    now.current = to;
    keep(to);
    return to.keys[to.at];
  };
  const b = back(history, shown, ok);
  const f = forward(history, shown, ok);
  return {
    back: b ? b.keys[b.at] : null,
    forward: f ? f.keys[f.at] : null,
    goBack: () => go(back(now.current, shown, ok)),
    goForward: () => go(forward(now.current, shown, ok)),
  };
}
