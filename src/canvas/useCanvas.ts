// A board's canvas document for the window: read from the engine when the board changes, kept up
// to date with what the board holds, and saved a moment after each change. A board whose canvas
// was never shown has no document until it's first shown; it then starts tidied.

import { useCallback, useEffect, useRef, useState } from 'react';
import { canvasLoad, canvasSave, onCanvasSaved, windowLabel } from '../api';
import { useLatest } from '../latest';
import { clearNotice, failedNotice, reportNotice } from '../notices';
import { addRef, DEFAULT_TIDY, emptyDoc, itemId, readDoc, refId, sync, tidy, type CanvasDoc, type Entities, type OnBoard, type RefKind } from './model';

const SAVE_AFTER_MS = 600;

interface Held {
  board: string;
  /** Null until the board's canvas is first shown. */
  doc: CanvasDoc | null;
}

export interface CanvasHandle {
  /** The document, or null while it loads or before the canvas was ever shown here. */
  doc: CanvasDoc | null;
  /** The board's document has been read: `doc` null then means it has none yet. */
  loaded: boolean;
  update: (fn: (doc: CanvasDoc) => CanvasDoc) => void;
  /** Replaces the document and saves it at once, for a change whose saving must be known (an
   *  arrange the command asked for, which says whether it was saved). */
  put: (doc: CanvasDoc) => Promise<void>;
  /** Puts a thing on the canvas and keeps it there; returns its canvas id. */
  keep: (ref: RefKind, key: string) => string;
  /** Whether a thing is on the canvas now. */
  has: (ref: RefKind, key: string) => boolean;
}

/** Each board's saves from this window: how many were asked, and the last one's promise. */
const saving = new Map<string, { asked: number; last: Promise<unknown> }>();

/** Saves a board's canvas, as every save from this window goes, for `readCanvas`. */
export function saveCanvas(board: string, doc: CanvasDoc): Promise<void> {
  const last = canvasSave(board, doc);
  saving.set(board, { asked: (saving.get(board)?.asked ?? 0) + 1, last: last.catch(() => null) });
  return last;
}

/** A board's canvas as its file has it once this window's saves of it are written, read again
 *  when the window asked for another save while it was read (it left the board, say, which saves
 *  the change it had still to save). After a few reads every one of which a save overtook, it
 *  fails rather than give what it knows is old. */
export async function readCanvas(board: string): Promise<CanvasDoc | null> {
  for (let tries = 0; tries < 4; tries++) {
    const before = saving.get(board);
    await before?.last;
    const raw = await canvasLoad(board);
    if (saving.get(board) === before) return raw ? readDoc(raw) : null;
  }
  throw new Error('The canvas kept changing while it was read. Ask again in a moment.');
}

/** The first document for a board: what's recent of what it holds, tidied by worktree. */
export function seed(e: Entities, now: number, pinned: Set<string>, on: OnBoard): CanvasDoc {
  return tidy(emptyDoc(), e, now, pinned, DEFAULT_TIDY, on);
}

/** `read` is false until the engine's first scan has finished: the board has only other devices'
 *  worktrees then, and a first tidy of those would leave out this Mac's. `on` is what the board
 *  holds (model.ts `OnBoard`). */
export function useCanvas(board: string, shown: boolean, read: boolean, e: Entities, now: number, pinned: Set<string>, on: OnBoard): CanvasHandle {
  const [held, setHeld] = useState<Held | null>(null);
  const loaded = held?.board === board;
  const ctx = useRef({ e, now, pinned, on });
  ctx.current = { e, now, pinned, on };

  // Saving: the latest document per board, written a moment after the last change.
  const pending = useRef<{ board: string; doc: CanvasDoc } | null>(null);
  const timer = useRef(0);
  // Counts the changes made here, each of which is saved: a read of another window's save that
  // one overtook would put back what came before it.
  const changes = useRef(0);
  // The board shown now, for a `put` asked before the window moved on to another.
  const boardNow = useLatest(board);
  const flush = useCallback(() => {
    window.clearTimeout(timer.current);
    const p = pending.current;
    pending.current = null;
    if (!p) return;
    saveCanvas(p.board, p.doc).then(
      () => clearNotice('canvas'),
      (e: unknown) => reportNotice(failedNotice('canvas', 'The canvas isn’t saving', 'Couldn’t save the canvas; it saves again with the next change', e)),
    );
  }, []);
  const save = useCallback(
    (b: string, doc: CanvasDoc) => {
      changes.current += 1;
      if (pending.current && pending.current.board !== b) flush();
      pending.current = { board: b, doc };
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(flush, SAVE_AFTER_MS);
    },
    [flush],
  );
  useEffect(() => {
    window.addEventListener('beforeunload', flush);
    return () => {
      window.removeEventListener('beforeunload', flush);
      flush();
    };
  }, [flush]);

  useEffect(() => {
    let live = true;
    flush();
    // What was put while it was read (an arrange, before the window first drew this board) is
    // newer than what it read.
    const at = changes.current;
    const land = (doc: CanvasDoc | null) => live && setHeld((h) => (h?.board === board && changes.current !== at ? h : { board, doc }));
    canvasLoad(board)
      .then((raw) => land(raw ? readDoc(raw) : null))
      .catch(() => land(null));
    return () => {
      live = false;
    };
  }, [board, flush]);

  // Another window saved this board's canvas (a second window on the project, or the main window
  // arranging it for `codealong board arrange`): read it again. A change made here that's still
  // to be saved goes over it instead, and the other window reads that.
  useEffect(
    () =>
      onCanvasSaved((saved) => {
        if (saved.from === windowLabel() || saved.board !== board || pending.current?.board === board) return;
        const at = changes.current;
        canvasLoad(board)
          // Asked again as it lands: a change made here meanwhile is newer, saved or to be saved.
          .then((raw) => setHeld((h) => (h?.board === board && raw && changes.current === at ? { board, doc: readDoc(raw) } : h)))
          // One that doesn't read now leaves this window with what it had, which it saves with its next change.
          .catch(() => {});
      }),
    [board],
  );

  // Brought up to date with what the board holds as it changes, once the canvas exists.
  useEffect(() => {
    if (!held || held.board !== board || !read) return;
    if (!held.doc) {
      if (!shown || e.lanes.size === 0) return;
      const doc = seed(e, now, pinned, on);
      setHeld({ board, doc });
      save(board, doc);
      return;
    }
    const next = sync(held.doc, e, now, pinned, on);
    if (next !== held.doc) {
      setHeld({ board, doc: next });
      save(board, next);
    }
  }, [held, board, shown, read, e, now, pinned, on, save]);

  const update = useCallback(
    (fn: (doc: CanvasDoc) => CanvasDoc) => {
      setHeld((h) => {
        if (!h || h.board !== board) return h;
        const { e: ents, now: at, pinned: pins, on: holds } = ctx.current;
        const before = h.doc ?? seed(ents, at, pins, holds);
        const doc = fn(before);
        if (doc === h.doc) return h;
        save(board, doc);
        return { board, doc };
      });
    },
    [board, save],
  );

  const put = useCallback(
    async (doc: CanvasDoc) => {
      changes.current += 1;
      // This board's change still to save is superseded; another's, the window having moved on
      // since this was asked, is saved as it would have been.
      if (pending.current?.board === board) {
        window.clearTimeout(timer.current);
        pending.current = null;
      }
      // The window's document while it still shows the board, whether or not its document has
      // been read yet: this is newer than what a read still out brings, which the read sees by
      // the count.
      if (boardNow.current === board) setHeld({ board, doc });
      await saveCanvas(board, doc);
      clearNotice('canvas');
    },
    [board, boardNow],
  );

  const keep = useCallback(
    (ref: RefKind, key: string) => {
      const id = refId(ref, key);
      update((d) => {
        const at = d.items.findIndex((it) => itemId(it) === id);
        if (at < 0) return addRef(d, ctx.current.e, ref, key, null, true);
        const it = d.items[at];
        if (it.kind !== 'ref' || it.kept) return d;
        const items = [...d.items];
        items[at] = { ...it, kept: true };
        return { ...d, items };
      });
      return id;
    },
    [update],
  );

  const doc = loaded ? (held?.doc ?? null) : null;
  const has = useCallback((ref: RefKind, key: string) => !!doc?.items.some((it) => it.kind === 'ref' && it.ref === ref && it.key === key), [doc]);
  return { doc, loaded, update, put, keep, has };
}
