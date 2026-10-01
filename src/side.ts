// Side conversations as a window knows them (side.rs): what the engine last said of each
// session's, kept for every box showing that session. A side conversation is a question about a
// session answered by a throwaway copy of its conversation, while the session carries on,
// unaware; it has one at a time, and it lasts until it's closed. Codealong keeps what was asked
// and answered, so the closed ones can be read again: a session's box shows them (`showSides`).

import { useCallback, useEffect, useState, useSyncExternalStore } from 'react';
import { onSide, sideGet, sidePast } from './api';
import type { Side, SideKept, SideTurn } from './types';

const sides = new Map<string, Side>();
const hearers = new Set<() => void>();
let listening = false;

const tell = () => {
  for (const hear of hearers) hear();
};

/** Takes what the engine says of a side conversation: one with no turns has ended. */
export function noteSide(side: Side): void {
  if (side.turns.length > 0) sides.set(side.key, side);
  else sides.delete(side.key);
  tell();
}

/** What a session's box shows in place of its open side conversation: the list of the ones
 *  kept, or one of them to read. Null shows the open one, if there is one. It's this window's. */
export type SideView = { kind: 'list' } | { kind: 'past'; id: string } | null;

const views = new Map<string, SideView>();

export function showSides(key: string, view: SideView): void {
  if (view) views.set(key, view);
  else views.delete(key);
  tell();
}

export function useSideView(key: string): SideView {
  return useSyncExternalStore(subscribe, () => views.get(key) ?? null);
}

/** The side conversations kept for a session, the latest first, read again whenever `stamp`
 *  changes (a turn ended, one closed) and when asked to (one was forgotten). */
export function usePast(key: string, stamp: string): [SideKept[], () => void] {
  const [past, setPast] = useState<SideKept[]>([]);
  const [asked, setAsked] = useState(0);
  useEffect(() => {
    let live = true;
    sidePast(key).then(
      (kept) => {
        if (live) setPast(kept);
      },
      () => {},
    );
    return () => {
      live = false;
    };
  }, [key, stamp, asked]);
  return [past, useCallback(() => setAsked((n) => n + 1), [])];
}

/** A kept side conversation in a line: its first question. */
export function keptLine(kept: SideKept): string {
  return kept.turns[0]?.question.split('\n')[0].trim() ?? '';
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
