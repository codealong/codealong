// Side conversations as a window knows them (side.rs): what the engine last said of each
// session's, kept for every box showing that session. A side conversation is a question about a
// session answered by a throwaway copy of its conversation, while the session carries on,
// unaware; it has one at a time, and it lasts until it's closed.

import { useEffect, useSyncExternalStore } from 'react';
import { onSide, sideGet } from './api';
import type { Side, SideTurn } from './types';

const sides = new Map<string, Side>();
const hearers = new Set<() => void>();
let listening = false;

/** Takes what the engine says of a side conversation: one with no turns has ended. */
export function noteSide(side: Side): void {
  if (side.turns.length > 0) sides.set(side.key, side);
  else sides.delete(side.key);
  for (const hear of hearers) hear();
}

function subscribe(hear: () => void): () => void {
  hearers.add(hear);
  return () => {
    hearers.delete(hear);
  };
}

/** A session's side conversation, as it's asked and answered; null when it has none. */
export function useSide(key: string): Side | null {
  useEffect(() => {
    if (!listening) {
      listening = true;
      onSide(noteSide);
    }
    // One begun before this window showed the session. What's been heard since is newer.
    let live = true;
    sideGet(key).then(
      (side) => {
        if (live && side && !sides.has(key)) noteSide(side);
      },
      () => {},
    );
    return () => {
      live = false;
    };
  }, [key]);
  return useSyncExternalStore(subscribe, () => sides.get(key) ?? null);
}

/** What a turn that hasn't its answer yet is doing, in a few words: the tool the copy runs is
 *  said by the turn itself, so this is for before and between. `first` is the turn the copy is
 *  on; the ones after it wait. */
export function turnState(turn: SideTurn, first: boolean): string | null {
  if (turn.done || turn.tool) return null;
  if (!first) return 'Next';
  return turn.answer ? null : 'Reading the conversation…';
}
