// A project's canvas document for the window: read from the engine when the project changes,
// kept up to date with the board, and saved a moment after each change. A project that has
// never had the Canvas view open has no document until it's first shown; it then starts tidied.

import { useCallback, useEffect, useRef, useState } from 'react';
import { canvasLoad, canvasSave, onCanvasSaved, windowLabel } from '../api';
import { clearNotice, failedNotice, reportNotice } from '../notices';
import { addRef, emptyDoc, itemId, readDoc, refId, sync, tidy, type CanvasDoc, type Entities, type RefKind } from './model';

const SAVE_AFTER_MS = 600;

interface Held {
  project: string;
  /** Null until the project's canvas is first shown. */
  doc: CanvasDoc | null;
}

export interface CanvasHandle {
  /** The document, or null while it loads or before the canvas was ever shown here. */
  doc: CanvasDoc | null;
  /** The project's document has been read: `doc` null then means it has none yet. */
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

/** The first document for a project: what's recent, tidied by worktree. */
export function seed(e: Entities, now: number, pinned: Set<string>): CanvasDoc {
  return tidy(emptyDoc(), e, now, pinned);
}

/** `read` is false until the engine's first scan has finished: the board has only other devices'
 *  worktrees then, and a first tidy of those would leave out this Mac's. */
export function useCanvas(project: string, shown: boolean, read: boolean, e: Entities, now: number, pinned: Set<string>): CanvasHandle {
  const [held, setHeld] = useState<Held | null>(null);
  const loaded = held?.project === project;
  const ctx = useRef({ e, now, pinned });
  ctx.current = { e, now, pinned };

  // Saving: the latest document per project, written a moment after the last change.
  const pending = useRef<{ project: string; doc: CanvasDoc } | null>(null);
  const timer = useRef(0);
  // Counts the changes made here, each of which is saved: a read of another window's save that
  // one overtook would put back what came before it.
  const changes = useRef(0);
  const flush = useCallback(() => {
    window.clearTimeout(timer.current);
    const p = pending.current;
    pending.current = null;
    if (!p) return;
    canvasSave(p.project, p.doc).then(
      () => clearNotice('canvas'),
      (e: unknown) => reportNotice(failedNotice('canvas', 'The canvas isn’t saving', `Couldn’t save the ${p.project} canvas; it saves again with the next change`, e)),
    );
  }, []);
  const save = useCallback(
    (p: string, doc: CanvasDoc) => {
      changes.current += 1;
      if (pending.current && pending.current.project !== p) flush();
      pending.current = { project: p, doc };
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
    canvasLoad(project)
      .then((raw) => live && setHeld({ project, doc: raw ? readDoc(raw) : null }))
      .catch(() => live && setHeld({ project, doc: null }));
    return () => {
      live = false;
    };
  }, [project, flush]);

  // Another window saved this project's canvas (a second window on the project, or the main
  // window arranging it for `codealong canvas arrange`): read it again. A change made here that's
  // still to be saved goes over it instead, and the other window reads that.
  useEffect(
    () =>
      onCanvasSaved((saved) => {
        if (saved.from === windowLabel() || saved.project !== project || pending.current?.project === project) return;
        const at = changes.current;
        canvasLoad(project)
          // Asked again as it lands: a change made here meanwhile is newer, saved or to be saved.
          .then((raw) => setHeld((h) => (h?.project === project && raw && changes.current === at ? { project, doc: readDoc(raw) } : h)))
          // One that doesn't read now leaves this window with what it had, which it saves with its next change.
          .catch(() => {});
      }),
    [project],
  );

  // Brought up to date with the board as it changes, once the canvas exists.
  useEffect(() => {
    if (!held || held.project !== project || !read) return;
    if (!held.doc) {
      if (!shown || e.lanes.size === 0) return;
      const doc = seed(e, now, pinned);
      setHeld({ project, doc });
      save(project, doc);
      return;
    }
    const next = sync(held.doc, e, now, pinned);
    if (next !== held.doc) {
      setHeld({ project, doc: next });
      save(project, next);
    }
  }, [held, project, shown, read, e, now, pinned, save]);

  const update = useCallback(
    (fn: (doc: CanvasDoc) => CanvasDoc) => {
      setHeld((h) => {
        if (!h || h.project !== project) return h;
        const { e: ents, now: at, pinned: pins } = ctx.current;
        const before = h.doc ?? seed(ents, at, pins);
        const doc = fn(before);
        if (doc === h.doc) return h;
        save(project, doc);
        return { project, doc };
      });
    },
    [project, save],
  );

  const put = useCallback(
    async (doc: CanvasDoc) => {
      // Another project's change still to save goes first; this project's is superseded.
      if (pending.current && pending.current.project !== project) flush();
      changes.current += 1;
      window.clearTimeout(timer.current);
      pending.current = null;
      setHeld((h) => (h?.project === project ? { project, doc } : h));
      await canvasSave(project, doc);
      clearNotice('canvas');
    },
    [project, flush],
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
