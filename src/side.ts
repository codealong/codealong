// Side conversations as a window knows them (side.rs): what the engine last said of each
// session's, kept for every box showing that session. A side conversation is a question about a
// session answered by a throwaway copy of its conversation, while the session carries on,
// unaware; it has one at a time, and it lasts until it's closed. Codealong keeps what was asked
// and answered, so the closed ones can be read again: a session's box shows them (`showSides`),
// from the one list kept here (`useKept`), and its header and its row in Sessions name each, as
// the board lists them (`Board.sides`), a click showing one (`showSide`). One closed while its
// copy answers is among them, the copy answering what it was asked before it ends, and reads as
// it answers (`useClosing`). A question asked while one is read goes on in it (`beingRead`): it's
// the open one again, the copy told what was said in it, and the one open before is closed.
//
// What shows above a session's box is the same in every window (`SideView`, kept in local
// storage, which they share), and so is whether the box is the side conversation's: a box in
// one window that sent to the session while another window's asked the copy once sent the
// session a question meant for the side (2026-10-06).
//
// A side conversation can pop out into a window of its own (kept.rs `pop_side`), which shows
// what would show above the box and has a box that only ever asks it. While it's out, nothing
// shows above the session's boxes, which are the session's again (`usePopped`).

import { useEffect, useSyncExternalStore } from 'react';
import { onSide, onSideWindows, sideForget, sideGet, sidePast, sideWindow, sideWindows } from './api';
import { errorText } from './report';
import { readStored, writeStored, type Codec } from './stored';
import type { Side, SideGlance, SideKept, SideTurn } from './types';

const sides = new Map<string, Side>();
/** The side conversations closed while their copies answered, by id, as the engine last said, until
 *  they've answered: a new map whenever one changes, for `useClosing`. */
let closing: ReadonlyMap<string, Side> = new Map();
/** How many questions each one closed had as it closed, and each one whose copy stopped as it
 *  stopped, by id. A closed one is open again only once it's asked again (side.rs `Reading`),
 *  with a question more, nor answering once it has stopped, so what says so with no more
 *  questions was sent before and came late: the engine tells of one from more than one thread
 *  (its copy's, a window's close). One forgotten is never back. */
const closedAt = new Map<string, number>();
const stoppedAt = new Map<string, number>();
/** Whether what's said of a side conversation was said before what `at` holds of it. */
const saidBefore = (at: ReadonlyMap<string, number>, side: Side) => side.turns.length <= (at.get(side.id) ?? -1);
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

/** The sessions whose side conversations have windows of their own, as the engine last said. */
let popped: ReadonlySet<string> = new Set();
let hearingPopped = false;

/** Hears which side conversations are in windows of their own, once a window shows a session:
 *  what the engine says as one pops out or closes, and what it says now, unless it's said more
 *  since. */
function hearPopped(): void {
  if (hearingPopped) return;
  hearingPopped = true;
  let heard = 0;
  const take = (keys: string[]) => {
    popped = new Set(keys);
    tell();
  };
  onSideWindows((keys) => {
    heard += 1;
    take(keys);
  });
  const before = heard;
  sideWindows().then(
    (keys) => {
      if (heard === before) take(keys);
    },
    // None read: none out, until one pops out and the engine says so.
    () => {},
  );
}

/** Whether a session's side conversation is in a window of its own. */
export function usePopped(key: string): boolean {
  return useSyncExternalStore(subscribe, () => popped.has(key));
}

/** How many of its turns have ended: each is kept as it does. */
const ended = (side: Side | undefined) => (side ? side.turns.filter((t) => t.done).length : 0);

/** How often each session's side conversation has been told of here, so a read that answers
 *  after it was told of again is dropped (`readSide`). */
const toldOf = new Map<string, number>();

/** Takes what the engine says of a side conversation: one with no turns has ended. What's kept
 *  of it changed if a turn ended or it closed, so that's read again. */
export function noteSide(side: Side): void {
  toldOf.set(side.key, (toldOf.get(side.key) ?? 0) + 1);
  if (side.closed) return noteClosing(side);
  if (side.turns.length > 0 && saidBefore(closedAt, side)) return;
  // Asked again as it was read, one closed is the open one.
  if (side.turns.length > 0 && closing.has(side.id)) {
    const next = new Map(closing);
    next.delete(side.id);
    closing = next;
  }
  const before = sides.get(side.key);
  if (side.turns.length > 0) sides.set(side.key, side);
  else sides.delete(side.key);
  if (side.turns.length === 0 || before?.id !== side.id || ended(before) !== ended(side)) readKept(side.key);
  // A question being asked has its copy now, or the copy is gone: the engine says which.
  if (viewOf(side.key)?.kind === 'starting') showSides(side.key, null);
  tell();
}

/** Takes what the engine says of one closed while its copy answered. Once it has answered, what's
 *  kept of it is what it said, until that's read again. */
function noteClosing(side: Side): void {
  if (saidBefore(stoppedAt, side)) return;
  // Open again with a question more since, as another window asked it.
  const open = sides.get(side.key);
  if (open?.id === side.id && open.turns.length > side.turns.length) return;
  closedAt.set(side.id, Math.max(closedAt.get(side.id) ?? 0, side.turns.length));
  // Closed in another window, its open copy may have come late, after the word that it's gone.
  if (sides.get(side.key)?.id === side.id) sides.delete(side.key);
  const before = closing.get(side.id);
  const next = new Map(closing);
  if (side.running) next.set(side.id, side);
  else {
    stoppedAt.set(side.id, side.turns.length);
    next.delete(side.id);
    const list = kept.get(side.key);
    if (list) kept.set(side.key, list.map((k) => (k.id === side.id ? { ...k, turns: side.turns, answering: false } : k)));
  }
  closing = next;
  if (!side.running || ended(before) !== ended(side)) readKept(side.key);
  tell();
}

/** The side conversations closed while their copies answered, by id, each as far as it's got since
 *  this window heard of it: read as they answer, as the kept ones are. One closed before is
 *  answering in what's kept (`SideKept.answering`) until it's heard of here. */
export function useClosing(): ReadonlyMap<string, Side> {
  useEffect(listen, []);
  return useSyncExternalStore(subscribe, () => closing);
}

/** What a session's box shows in place of its open side conversation: the list of the ones
 *  kept, or one of them to read, or the question just asked (`starting`, at `at`), until the
 *  engine says its copy has started, which takes seconds on a busy Mac. Null shows the open one,
 *  if there is one. Every window shows the same. */
export type SideView = { kind: 'list' } | { kind: 'past'; id: string } | { kind: 'starting'; at: number } | null;

/** Each session's view is an item of its own in local storage. */
const VIEW = 'codealong.sideview.';

/** How long a question waits for the engine to say its copy started before its box is the
 *  session's again: a window that quit while one was starting never hears. */
export const STARTING_MS = 2 * 60 * 1000;

/** A view as it's kept: JSON. What doesn't read as one reads as none. */
export const viewCodec: Codec<SideView> = {
  parse: (raw) => {
    const v: unknown = JSON.parse(raw);
    if (typeof v !== 'object' || v === null) return undefined;
    const o = v as Record<string, unknown>;
    if (o.kind === 'list') return { kind: 'list' };
    if (o.kind === 'past' && typeof o.id === 'string') return { kind: 'past', id: o.id };
    if (o.kind === 'starting' && typeof o.at === 'number') return { kind: 'starting', at: o.at };
    return undefined;
  },
  format: (view) => (view ? JSON.stringify(view) : null),
};

/** The views read so far, by session: null for one read and found to be none. */
const views = new Map<string, SideView>();
let watching = false;

/** Takes another window's changes to the views: each is read again when next looked at. */
function watchViews(): void {
  if (watching || typeof window === 'undefined') return;
  watching = true;
  window.addEventListener('storage', (e) => {
    // No key: storage was cleared.
    if (e.key === null) views.clear();
    else if (e.key.startsWith(VIEW)) views.delete(e.key.slice(VIEW.length));
    else return;
    tell();
  });
}

function viewOf(key: string): SideView {
  if (!views.has(key)) views.set(key, readStored(VIEW + key, viewCodec, null));
  return views.get(key) ?? null;
}

/** Shows what's above a session's box, in every window. */
export function showSides(key: string, view: SideView): void {
  views.set(key, view);
  writeStored(VIEW + key, viewCodec, view);
  // Read again as they're shown: another window may have deleted one.
  if (view && view.kind !== 'starting') readKept(key);
  tell();
}

/** Shows one of a session's side conversations, as its row under the session or its line in the
 *  session's header asks: the open one above its box, or one kept to read there. Out in a window
 *  of its own, that window shows it and comes forward. */
export function showSide(key: string, side: Pick<SideGlance, 'id' | 'open'>): void {
  showSides(key, side.open ? null : { kind: 'past', id: side.id });
  // Up already, so it can't fail but for a window that's gone meanwhile, which the next
  // `side-windows` takes out of `popped`.
  if (popped.has(key)) void sideWindow(key, true).catch(() => {});
}

/** The side conversation a question asked now goes on in rather than the open one: the one being
 *  read above the box, if one is (side.rs `Reading`). Asked there, a question is for what you
 *  read, not for a new copy that knows none of it (2026-10-25). */
export function beingRead(key: string): string | null {
  const view = viewOf(key);
  return view?.kind === 'past' ? view.id : null;
}

/** A question goes to a session's side conversation: its box stays the side conversation's
 *  while the engine starts the copy, in every window. */
export function startingSide(key: string): void {
  showSides(key, { kind: 'starting', at: Date.now() });
}

/** A question that couldn't be asked: its box stays the side conversation's, with the open one
 *  above it, or with none open, the ones kept. */
export function notAsked(key: string): void {
  if (viewOf(key)?.kind !== 'starting') return;
  showSides(key, sides.has(key) ? null : { kind: 'list' });
}

export function useSideView(key: string): SideView {
  return useSyncExternalStore(subscribe, () => viewOf(key));
}

/** Whether a view gives a session's box to its side conversation: any, but a question's start
 *  only while it may still come. */
export function takesBox(view: SideView, now: number): boolean {
  return !!view && (view.kind !== 'starting' || now - view.at < STARTING_MS);
}

/** Whether anything of a session's side conversations shows above its box, in this window and
 *  every other: the open one (answering or ended), the list, one being read, or a question
 *  starting one. While it does, what's typed in the box is for the side conversation, as Codex's
 *  `/side` takes its box until you go back. Not while it's in a window of its own, whose box
 *  asks it. */
export function useAside(key: string): boolean {
  return useSyncExternalStore(subscribe, () => !popped.has(key) && (sides.has(key) || takesBox(viewOf(key), Date.now())));
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
  const told = toldOf.get(key) ?? 0;
  sidePast(key).then(
    (list) => {
      if (reads.get(key) !== read) return;
      kept.set(key, list);
      // Unless it was told of meanwhile, which is newer than the read.
      if ((toldOf.get(key) ?? 0) === told) answeredSince(key, list);
      tell();
    },
    () => {},
  );
}

/** Lets go of the session's ones closed while their copies answered that what's kept says have
 *  answered, or has no more: another machine's may have, while the two weren't connected, with
 *  nothing told of it here. Every one still answering is kept as answering (side.rs `past`). */
function answeredSince(key: string, list: SideKept[]): void {
  const done = [...closing.values()].filter((c) => c.key === key && !list.some((k) => k.id === c.id && k.answering));
  if (done.length === 0) return;
  const next = new Map(closing);
  for (const c of done) {
    stoppedAt.set(c.id, c.turns.length);
    next.delete(c.id);
  }
  closing = next;
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

/** Closes a side conversation: it's gone from every box at once, and the engine told (`close`, a
 *  session's or a PR's), whose copy answers what it was asked before it ends, read meanwhile as
 *  the kept ones are (`useClosing`). What was said is kept. Closing one here can't fail
 *  (side.rs); one on another machine that didn't hear it is still open there, so it's back above
 *  the box as that machine has it, and `failed` hears why. */
export function endSide(key: string, close: (key: string) => Promise<void>, failed?: (why: string) => void): void {
  const side = sides.get(key);
  if (!side) return;
  const was = closedAt.get(side.id);
  closedAt.set(side.id, side.turns.length);
  noteSide({ id: side.id, key, turns: [], running: false, ended: true });
  close(key).catch((e: unknown) => {
    if (was === undefined) closedAt.delete(side.id);
    else closedAt.set(side.id, was);
    noteSide(side);
    readSide(key);
    failed?.(errorText(e));
  });
}

/** Forgets a kept side conversation, or stops one closed and still answering: it's off the list
 *  at once. */
export function forgetSide(key: string, id: string): void {
  kept.set(
    key,
    (kept.get(key) ?? NONE).filter((k) => k.id !== id),
  );
  stoppedAt.set(id, Infinity);
  if (closing.has(id)) {
    const next = new Map(closing);
    next.delete(id);
    closing = next;
  }
  tell();
  const again = () => readKept(key);
  sideForget(key, id).then(again, again);
}

/** A kept side conversation in a line: its first question. */
export function keptLine(kept: SideKept): string {
  return kept.turns[0]?.question.split('\n')[0].trim() ?? '';
}

function subscribe(hear: () => void): () => void {
  watchViews();
  hearPopped();
  hearers.add(hear);
  return () => {
    hearers.delete(hear);
  };
}

/** Reads a session's side conversation again, as the engine has it now, unless it's told of it
 *  meanwhile, which is newer. One on another machine may have gone on, or closed, while the two
 *  weren't connected, with nothing told of it here. */
function readSide(key: string): void {
  const before = toldOf.get(key) ?? 0;
  sideGet(key).then(
    (side) => {
      if ((toldOf.get(key) ?? 0) !== before) return;
      const had = sides.get(key);
      if (side) noteSide(side);
      else if (had) noteSide({ id: had.id, key, turns: [], running: false, ended: true });
    },
    // Unread, it shows as it was told of.
    () => {},
  );
}

/** A session's side conversation, as it's asked and answered; null when it has none. Read as a
 *  window shows the session: one begun before, or one gone on elsewhere. */
export function useSide(key: string): Side | null {
  useEffect(() => {
    listen();
    readSide(key);
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
