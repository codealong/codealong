// The sessions kept in windows of their own (kept.rs): popped out of the main window's panel, or
// kept from the peek or the notch. A window that shows sessions in panels follows which they are,
// read once and then heard as one pops out or closes, and shows a kept one's place, with the ways
// to its window and back, rather than a second copy of its conversation and box
// (SessionPanel.tsx `KeptAway`).

import { useSyncExternalStore } from 'react';
import { onSessionWindows, sessionWindows } from './api';

const NONE: ReadonlySet<string> = new Set();

let kept: ReadonlySet<string> = NONE;
let following = false;
const listeners = new Set<() => void>();

/** Starts following, once: what the engine says as one pops out or closes, and what it says now,
 *  asked once that's heard so no change falls between the two, and dropped when it's said more
 *  since it was asked. */
function follow(): void {
  if (following) return;
  following = true;
  let heard = 0;
  const take = (keys: string[]) => {
    kept = new Set(keys);
    for (const l of listeners) l();
  };
  onSessionWindows(
    (keys) => {
      heard += 1;
      take(keys);
    },
    () => {
      const before = heard;
      sessionWindows().then(
        (keys) => {
          if (heard === before) take(keys);
        },
        // None read: none kept, as far as this window knows, until one pops out and the engine says so.
        () => {},
      );
    },
  );
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** The sessions kept in windows of their own, as that changes; none where `on` is false (a card,
 *  which is such a window or shows one only for a moment), which follows nothing for it. */
export function useKeptSessions(on = true): ReadonlySet<string> {
  if (on) follow();
  return useSyncExternalStore(subscribe, () => (on ? kept : NONE));
}
