// A project's canvas document for the window: read from the engine when the project changes,
// kept up to date with the board, and saved a moment after each change. A project that has
// never had the Canvas view open has no document until it's first shown; it then starts tidied.

import { useCallback, useEffect, useRef, useState } from 'react';
import { canvasLoad, canvasSave } from '../api';
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
  update: (fn: (doc: CanvasDoc) => CanvasDoc) => void;
  /** Puts a thing on the canvas and keeps it there; returns its canvas id. */
  keep: (ref: RefKind, key: string) => string;
  /** Whether a thing is on the canvas now. */
  has: (ref: RefKind, key: string) => boolean;
}

/** The first document for a project: what's recent, tidied by worktree. */
function seed(e: Entities, now: number, pinned: Set<string>): CanvasDoc {
  return tidy(sync(emptyDoc(), e, now, pinned), e, 'worktree', now);
}

export function useCanvas(project: string, shown: boolean, e: Entities, now: number, pinned: Set<string>): CanvasHandle {
  const [held, setHeld] = useState<Held | null>(null);
  const loaded = held?.project === project;
  const ctx = useRef({ e, now, pinned });
  ctx.current = { e, now, pinned };

  // Saving: the latest document per project, written a moment after the last change.
  const pending = useRef<{ project: string; doc: CanvasDoc } | null>(null);
  const timer = useRef(0);
  const flush = useCallback(() => {
    window.clearTimeout(timer.current);
    const p = pending.current;
    pending.current = null;
    if (p) canvasSave(p.project, p.doc).catch(() => {});
  }, []);
  const save = useCallback(
    (p: string, doc: CanvasDoc) => {
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

  // Brought up to date with the board as it changes, once the canvas exists.
  useEffect(() => {
    if (!held || held.project !== project) return;
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
  }, [held, project, shown, e, now, pinned, save]);

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
  return { doc, update, keep, has };
}
