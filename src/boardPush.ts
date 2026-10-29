// What the engine pushes the windows of each board (windowfeed.rs): the first whole, then only
// what changed since the board before, which is put back together here into the whole board. A
// push for a board other than the one a window has (one it missed, or one from before it
// listened) can't be put together: the window reads the board whole instead.

import type { Board, Lane, PrItem, Session } from './types';

/** A lane without its sessions. */
type LaneHead = Omit<Lane, 'sessions'>;

export type BoardPush =
  | { rev: number; full: Board }
  | {
      rev: number;
      /** The board this changes. */
      base: number;
      /** The board's own fields that changed, but its lanes and PRs. */
      fields?: Partial<Board>;
      /** Fields the board no longer has. */
      gone?: string[];
      /** Every lane's id, in order, when that changed. */
      laneOrder?: string[];
      /** Lanes that changed, without their sessions. */
      lanes?: LaneHead[];
      /** Each session's key, in order, for the lanes where that changed. */
      sessionsOf?: Record<string, string[]>;
      /** Sessions that changed, by their lane's id. */
      sessions?: Record<string, Session[]>;
      /** Every PR's key, in order, when that changed. */
      prOrder?: string[];
      prs?: PrItem[];
    };

/** The board `push` makes of `prev`, or null when it can't be made from `prev` (it changes
 *  another board, or names a session `prev` doesn't have). Lanes, sessions and PRs that didn't
 *  change are `prev`'s own objects. */
export function applyPush(prev: Board | null, push: BoardPush): Board | null {
  if ('full' in push) return push.full;
  if (!prev || (prev.rev ?? 0) !== push.base) return null;
  const lanes = lanesOf(prev.lanes, push);
  const prs = prsOf(prev.prs, push);
  if (!lanes || !prs) return null;
  const next = { ...prev, ...push.fields, rev: push.rev, lanes, prs };
  for (const name of push.gone ?? []) delete (next as Record<string, unknown>)[name];
  return next;
}

function lanesOf(prev: Lane[], push: Exclude<BoardPush, { full: Board }>): Lane[] | null {
  if (!push.laneOrder && !push.lanes && !push.sessionsOf && !push.sessions) return prev;
  const was = new Map(prev.map((l) => [l.id, l]));
  const heads = new Map((push.lanes ?? []).map((h) => [h.id, h]));
  const out: Lane[] = [];
  for (const id of push.laneOrder ?? prev.map((l) => l.id)) {
    const old = was.get(id);
    const head = heads.get(id);
    const keys = push.sessionsOf?.[id];
    const changed = push.sessions?.[id];
    if (!head && !keys && !changed) {
      if (!old) return null;
      out.push(old);
      continue;
    }
    const sessions = sessionsOf(old?.sessions ?? [], keys, changed);
    const lane = head ?? old;
    if (!sessions || !lane) return null;
    out.push({ ...lane, sessions });
  }
  return out;
}

function sessionsOf(prev: Session[], keys: string[] | undefined, changed: Session[] | undefined): Session[] | null {
  const fresh = new Map((changed ?? []).map((s) => [s.key, s]));
  if (!keys) return prev.map((s) => fresh.get(s.key) ?? s);
  const had = new Map(prev.map((s) => [s.key, s]));
  const out: Session[] = [];
  for (const key of keys) {
    const s = fresh.get(key) ?? had.get(key);
    if (!s) return null;
    out.push(s);
  }
  return out;
}

function prsOf(prev: PrItem[], push: Exclude<BoardPush, { full: Board }>): PrItem[] | null {
  if (!push.prOrder && !push.prs) return prev;
  const fresh = new Map((push.prs ?? []).map((p) => [p.key, p]));
  if (!push.prOrder) return prev.map((p) => fresh.get(p.key) ?? p);
  const had = new Map(prev.map((p) => [p.key, p]));
  const out: PrItem[] = [];
  for (const key of push.prOrder) {
    const p = fresh.get(key) ?? had.get(key);
    if (!p) return null;
    out.push(p);
  }
  return out;
}

/** A window's pushes put together: the board the next one changes, and every board handed on
 *  as it comes. A push it can't put together has the board read whole; pushes that come
 *  meanwhile wait for it, and the ones it doesn't already have change it once it's in. */
export class Pushes {
  private board: Board | null = null;
  /** The newest board handed on, by its rev. */
  private handed = -1;
  private reading = false;
  private held: BoardPush[] = [];

  constructor(
    private readonly read: () => Promise<Board>,
    private readonly hand: (board: Board) => void,
  ) {}

  /** A board the window read whole: what the next push changes, unless one newer is here. */
  saw(board: Board): void {
    if (!this.board || (board.rev ?? 0) >= (this.board.rev ?? 0)) this.board = board;
  }

  take(push: BoardPush): void {
    if (this.reading) {
      this.held.push(push);
      return;
    }
    const board = this.board;
    // Here already, read whole before its push came: handed on now if it wasn't.
    if (board && push.rev <= (board.rev ?? 0)) {
      this.handOn(board);
      return;
    }
    const next = applyPush(board, push);
    if (next) {
      this.board = next;
      this.handOn(next);
    } else this.readWhole(push);
  }

  private handOn(board: Board): void {
    if ((board.rev ?? 0) <= this.handed) return;
    this.handed = board.rev ?? 0;
    this.hand(board);
  }

  private readWhole(push: BoardPush): void {
    this.reading = true;
    this.held = [push];
    this.read().then(
      (whole) => {
        this.reading = false;
        this.saw(whole);
        const held = this.held;
        this.held = [];
        // Read after they were sent, so it's at least as new as they are, but for any that
        // came after it went out.
        for (const p of held) {
          if (this.reading) this.held.push(p);
          else this.take(p);
        }
        if (this.board) this.handOn(this.board);
      },
      // What came meanwhile that fits the board here still goes on (a whole board always does),
      // without reading again; the next push that doesn't fit tries again.
      () => {
        this.reading = false;
        const held = this.held;
        this.held = [];
        for (const p of held) {
          if (this.board && p.rev <= (this.board.rev ?? 0)) continue;
          this.board = applyPush(this.board, p) ?? this.board;
        }
        if (this.board) this.handOn(this.board);
      },
    );
  }
}
