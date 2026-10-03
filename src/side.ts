// Side conversations as a window knows them (side.rs): what the engine last said of each
// session's, kept for every box showing that session. A side conversation is a question about a
// session answered by a throwaway copy of its conversation, while the session carries on,
// unaware; it has one at a time, and it lasts until it's closed. Codealong keeps what was asked
// and answered, so the closed ones can be read again: a session's box shows them (`showSides`),
// and its header says how many there are (`useKept`), from the one list kept here.

import { useEffect, useSyncExternalStore } from 'react';
import { onSide, sideForget, sideGet, sidePast } from './api';
import type { Side, SideKept, SideTurn } from './types';

const sides = new Map<string, Side>();
const hearers = new Set<() => void>();
let listening = false;

const tell = () => {
  for (const hear of hearers) hear();
};

/** Hears of side conversations from the engine, once a window shows a session. */
function listen(): void {
  if (listening) return;
  listening = true;
  onSide(noteSide);
}

/** How many of its turns have ended: each is kept as it does. */
const ended = (side: Side | undefined) => (side ? side.turns.filter((t) => t.done).length : 0);

/** Takes what the engine says of a side conversation: one with no turns has ended. What's kept
 *  of it changed if a turn ended or it closed, so that's read again. */
export function noteSide(side: Side): void {
  const before = sides.get(side.key);
  if (side.turns.length > 0) sides.set(side.key, side);
  else sides.delete(side.key);
  if (side.turns.length === 0 || before?.id !== side.id || ended(before) !== ended(side)) readKept(side.key);
  tell();
}

/** What a session's box shows in place of its open side conversation: the list of the ones
 *  kept, or one of them to read. Null shows the open one, if there is one. It's this window's. */
export type SideView = { kind: 'list' } | { kind: 'past'; id: string } | null;

const views = new Map<string, SideView>();

export function showSides(key: string, view: SideView): void {
  if (view) views.set(key, view);
  else views.delete(key);
  // Read again as they're shown: another window may have deleted one.
  if (view) readKept(key);
  tell();
}

export function useSideView(key: string): SideView {
  return useSyncExternalStore(subscribe, () => views.get(key) ?? null);
}

/** Whether anything of a session's side conversations shows above its box in this window: the
 *  open one (answering or ended), the list, or one being read. While it does, what's typed in the
 *  box is for the side conversation, as Codex's `/side` takes its box until you go back. */
export function useAside(key: string): boolean {
  return useSyncExternalStore(subscribe, () => sides.has(key) || views.has(key));
}

/** The side conversations kept for each session this window has shown, the latest first. */
const kept = new Map<string, SideKept[]>();
const NONE: SideKept[] = [];
/** The latest read asked for, by session: an earlier one that answers late is dropped. */
const reads = new Map<string, number>();

/** Reads what's kept for a session again. */
export function readKept(key: string): void {
  const read = (reads.get(key) ?? 0) + 1;
  reads.set(key, read);
  sidePast(key).then(
    (list) => {
      if (reads.get(key) !== read) return;
      kept.set(key, list);
      tell();
    },
    () => {},
  );
}

/** The side conversations kept for a session, the latest first: the open one too, as far as it's
 *  been answered. The same list for whatever shows it (the session's header, its box). */
export function useKept(key: string): SideKept[] {
  useEffect(() => {
    listen();
    readKept(key);
  }, [key]);
  return useSyncExternalStore(subscribe, () => kept.get(key) ?? NONE);
}

/** Forgets a kept side conversation: it's off the list at once. */
export function forgetSide(key: string, id: string): void {
  kept.set(
    key,
    (kept.get(key) ?? NONE).filter((k) => k.id !== id),
  );
  tell();
  const again = () => readKept(key);
  sideForget(id).then(again, again);
}

/** What a session's header says of its side conversations, and what a click on it shows: the
 *  list, or with only one from before and none open, that one to read. None kept: nothing. */
export function keptSummary(kept: SideKept[], open: Side | null): { words: string; show: SideView } | null {
  if (kept.length === 0) return null;
  const earlier = kept.filter((k) => k.id !== open?.id);
  const show: SideView = earlier.length === 0 ? null : earlier.length === 1 && !open ? { kind: 'past', id: earlier[0].id } : { kind: 'list' };
  return { words: kept.length === 1 ? '1 side conversation' : `${kept.length} side conversations`, show };
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
    listen();
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
