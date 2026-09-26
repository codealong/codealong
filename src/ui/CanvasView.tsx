// The Canvas view: the project's work laid out in space, to be remembered by where it is.
//
// Two layers share one camera. Underneath, WebGL draws the dot grid, the lines between related
// cards and, zoomed out past reading, every card as a block (canvas/gl.ts). On top, HTML draws
// what you read and touch: frames, notes and, zoomed in, the cards on screen and a little
// around it. Panning and zooming move the HTML layer with one transform, set straight on the
// element, so React doesn't render while the camera moves; cards render only when the cards
// in view change or their data does.
//
// Controls follow Figma: scroll or drag with Space held (or the middle button) to pan, pinch or
// ⌘-scroll to zoom, drag on empty canvas to select, ⇧1 to see everything, ⇧2 the selection,
// ⇧0 for 100%. N adds a note, F a frame. Double-click a card to open it where it lives.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type MouseEvent as ReactMouseEvent, type MutableRefObject, type PointerEvent as ReactPointerEvent } from 'react';
import { showContextMenu, type MenuSpec } from '../contextMenu';
import { CanvasGl, parseColor, type Block, type Line, type Rgba } from '../canvas/gl';
import {
  addRef,
  bounds,
  CARD_W,
  contains,
  itemId,
  links,
  overlaps,
  rectOf,
  refId,
  tidy,
  tidyPlan,
  clearDoc,
  titleOf,
  type Camera,
  type CanvasDoc,
  type CanvasItem,
  type Entities,
  type FrameItem,
  type NoteItem,
  type Rect,
  type RefItem,
  type RefKind,
  type Scope,
  type TidyPlan,
} from '../canvas/model';
import { COLOR_WORD, newId } from '../todos';
import { LABEL_COLORS, type Label, type LabelColor, type Theme, type Todo } from '../types';
import { CanvasCard, CardDetail, cardData, cardSig } from './CanvasCard';
import { TidyPanel, tidySummary } from './CanvasTidy';
import { Icon } from './primitives';

/** At or above this zoom, cards are HTML you can read; below it, blocks on the GPU. */
const NEAR = 0.4;
const MIN_ZOOM = 0.04;
const MAX_ZOOM = 4;
const HOVER_MS = 380;

type Tool = 'select' | 'note' | 'frame';

type Drag =
  | { mode: 'pan'; sx: number; sy: number; cam: Camera }
  | { mode: 'move'; sx: number; sy: number; moved: boolean; start: Map<string, { x: number; y: number }>; toggle: string | null }
  | { mode: 'marquee'; sx: number; sy: number; add: Set<string> }
  | { mode: 'frame'; sx: number; sy: number }
  | { mode: 'resize'; id: string; sx: number; sy: number; w: number; h: number };

export interface CanvasViewProps {
  doc: CanvasDoc;
  update: (fn: (doc: CanvasDoc) => CanvasDoc) => void;
  entities: Entities;
  /** Pinned worktrees, which stay on the canvas whatever their age. */
  pinned: Set<string>;
  labels: Map<string, Label>;
  todos: Todo[];
  now: number;
  terminal: string;
  theme: Theme;
  project: string;
  /** Select this item and bring it into view; `n` changes each time it's asked. */
  focus: { id: string; n: number } | null;
  onOpen: (it: RefItem) => void;
  /** The item's own right-click menu, as the other views show it. */
  menuFor: (it: RefItem) => MenuSpec;
  onToast: (message: string) => void;
  /** A toast with Undo. */
  onUndo: (message: string, undo: () => void, icon?: string) => void;
  /** The canvas's keys; true when it took the key. The window calls this before its own. */
  keys: MutableRefObject<(e: KeyboardEvent) => boolean>;
  /** The filter at the top of the window: cards and notes that don't match it fade. Empty for none. */
  query: string;
  /** Whether the text filter or an Inbox filter is on. */
  filtering: boolean;
  /** Whether a card matches them, as its row would in its own view. */
  matches: (it: RefItem) => boolean;
  /** Zooms to fit what matches each time it changes. */
  fitMatches: number;
  /** The one card selected, for the panel beside the canvas and ⌘N; null for none, several, or a note or frame. */
  onSelectItem: (it: RefItem | null) => void;
}

function readColors() {
  const css = getComputedStyle(document.documentElement);
  const v = (name: string, a = 1): Rgba => parseColor(css.getPropertyValue(name) || '#888', a);
  return {
    grid: v('--line-strong', 0.28),
    line: v('--ink-3', 0.55),
    needs: v('--attn-solid'),
    error: v('--alert-solid'),
    working: v('--logo-working'),
    unread: v('--unread-solid'),
    idle: v('--line-strong', 0.55),
    lane: v('--ink-2', 0.9),
    todo: v('--label-gray', 0.55),
    progress: v('--unread-solid', 0.7),
    done: v('--pr-open-solid', 0.45),
    open: v('--pr-open-solid', 0.8),
    merged: v('--pr-merged-solid', 0.8),
    gone: v('--line-strong', 0.3),
  };
}

const cx = (...parts: (string | false | null | undefined)[]) => parts.filter(Boolean).join(' ');
const clamp = (z: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));

export function CanvasView(props: CanvasViewProps) {
  const { doc, update, entities, pinned, labels, todos, now, terminal, theme, project, focus, onOpen, menuFor, onToast, onUndo, keys, onSelectItem, query, filtering, matches, fitMatches } = props;
  const rootRef = useRef<HTMLDivElement>(null);
  const worldRef = useRef<HTMLDivElement>(null);
  const glCanvas = useRef<HTMLCanvasElement>(null);
  const gl = useRef<CanvasGl | null>(null);
  const cam = useRef<Camera>(doc.camera ?? { x: 0, y: 0, zoom: 1 });
  const fitted = useRef(!!doc.camera);
  const [near, setNear] = useState(cam.current.zoom >= NEAR);
  const zoomLabel = useRef<HTMLButtonElement>(null);
  // Only whether names show zoomed out, so a zoom doesn't render the view at every step.
  const [farNames, setFarNames] = useState(cam.current.zoom >= 0.08);
  const [cull, setCull] = useState<Rect>({ x: -1e5, y: -1e5, w: 0, h: 0 });
  const [selection, setSelection] = useState<Set<string>>(() => new Set());
  const [hover, setHover] = useState<string | null>(null);
  const [popover, setPopover] = useState<string | null>(null);
  const [tool, setTool] = useState<Tool>('select');
  const [marquee, setMarquee] = useState<Rect | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const [spaceHeld, setSpaceHeld] = useState(false);
  const [moving, setMoving] = useState(false);
  const [glError, setGlError] = useState<string | null>(null);
  const drag = useRef<Drag | null>(null);
  const hoverTimer = useRef(0);
  const idleTimer = useRef(0);
  const gestureZoom = useRef<{ zoom: number; x: number; y: number } | null>(null);

  const items = doc.items;
  const byId = useMemo(() => new Map(items.map((it) => [itemId(it), it])), [items]);
  const refs = useMemo(() => items.filter((it): it is RefItem => it.kind === 'ref'), [items]);
  const frames = useMemo(() => items.filter((it): it is FrameItem => it.kind === 'frame'), [items]);
  const notes = useMemo(() => items.filter((it): it is NoteItem => it.kind === 'note'), [items]);
  const data = useMemo(() => new Map(refs.map((it) => [itemId(it), cardData(entities, it, labels)])), [refs, entities, labels]);
  const pairs = useMemo(() => links(entities, new Set(refs.map(itemId))), [entities, refs]);

  // What's tied to the hovered or selected cards, for the lines and the cards' highlight.
  const related = useMemo(() => {
    const from = new Set(selection);
    if (hover) from.add(hover);
    const out = new Set<string>();
    if (from.size === 0) return out;
    for (const [a, b] of pairs) {
      if (from.has(a)) out.add(b);
      if (from.has(b)) out.add(a);
    }
    for (const id of from) out.delete(id);
    return out;
  }, [pairs, selection, hover]);

  // What the filters fade: cards that don't match, and notes unless their text has what's typed.
  const faded = useMemo(() => {
    if (!filtering) return null;
    const q = query.trim().toLowerCase();
    const out = new Set<string>();
    for (const it of refs) if (!matches(it)) out.add(itemId(it));
    for (const n of notes) if (!q || !n.text.toLowerCase().includes(q)) out.add(n.id);
    return out;
  }, [filtering, query, refs, notes, matches]);

  /* ---- Camera ---- */

  const viewport = useCallback((): Rect => {
    const el = rootRef.current;
    const c = cam.current;
    const w = el?.clientWidth ?? 0;
    const h = el?.clientHeight ?? 0;
    return { x: -c.x / c.zoom, y: -c.y / c.zoom, w: w / c.zoom, h: h / c.zoom };
  }, []);

  const saveCamera = useCallback(() => {
    const c = { ...cam.current };
    update((d) => (d.camera && d.camera.x === c.x && d.camera.y === c.y && d.camera.zoom === c.zoom ? d : { ...d, camera: c }));
  }, [update]);

  const applyCamera = useCallback(
    (settle = true) => {
      const c = cam.current;
      const w = worldRef.current;
      if (w) {
        w.style.transform = `translate(${c.x}px, ${c.y}px) scale(${c.zoom})`;
        w.style.setProperty('--z', String(c.zoom));
      }
      gl.current?.request(() => cam.current, () => cam.current.zoom < NEAR);
      setNear(c.zoom >= NEAR);
      if (zoomLabel.current) zoomLabel.current.textContent = `${Math.round(c.zoom * 100)}%`;
      setFarNames(c.zoom >= 0.08);
      // Cards are drawn for the view and half a view around it, so a pan redraws them only when
      // it leaves that.
      const v = viewport();
      setCull((old) => {
        const inside = v.x >= old.x && v.y >= old.y && v.x + v.w <= old.x + old.w && v.y + v.h <= old.y + old.h && old.w < v.w * 3;
        return inside ? old : { x: v.x - v.w / 2, y: v.y - v.h / 2, w: v.w * 2, h: v.h * 2 };
      });
      setPopover(null);
      if (!settle) return;
      // While the camera moves, the layer is a GPU texture; a moment after, it's drawn crisply
      // at the new scale.
      setMoving(true);
      window.clearTimeout(idleTimer.current);
      idleTimer.current = window.setTimeout(() => {
        setMoving(false);
        saveCamera();
      }, 180);
    },
    [viewport, saveCamera],
  );

  const zoomAt = useCallback(
    (sx: number, sy: number, zoom: number) => {
      const c = cam.current;
      const z = clamp(zoom);
      const wx = (sx - c.x) / c.zoom;
      const wy = (sy - c.y) / c.zoom;
      cam.current = { zoom: z, x: sx - wx * z, y: sy - wy * z };
      applyCamera();
    },
    [applyCamera],
  );

  const fit = useCallback(
    (r: Rect | null, maxZoom = 1) => {
      const el = rootRef.current;
      if (!r || !el || el.clientWidth === 0) return false;
      const pad = 64;
      const z = clamp(Math.min(maxZoom, (el.clientWidth - pad * 2) / Math.max(r.w, 1), (el.clientHeight - pad * 2) / Math.max(r.h, 1)));
      cam.current = { zoom: z, x: el.clientWidth / 2 - (r.x + r.w / 2) * z, y: el.clientHeight / 2 - (r.y + r.h / 2) * z };
      applyCamera();
      return true;
    },
    [applyCamera],
  );

  const fitAll = useCallback(() => fit(bounds(items.map(rectOf))), [fit, items]);
  const fitSelection = useCallback(() => fit(bounds([...selection].flatMap((id) => (byId.get(id) ? [rectOf(byId.get(id) as CanvasItem)] : []))), 2), [fit, selection, byId]);

  // The WebGL layer, made once. Without WebGL the canvas still works: zoomed out, cards stay HTML.
  useEffect(() => {
    if (!glCanvas.current) return;
    try {
      gl.current = new CanvasGl(glCanvas.current);
    } catch (e) {
      setGlError(String(e));
    }
    return () => {
      gl.current?.dispose();
      gl.current = null;
    };
  }, []);

  useLayoutEffect(() => {
    applyCamera(false);
    const el = rootRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      if (!fitted.current && fitAll()) fitted.current = true;
      else applyCamera(false);
    });
    ro.observe(el);
    return () => ro.disconnect();
    // Once: the camera is the view's own from here on.
  }, []);

  // A project's own camera when the project changes.
  const lastProject = useRef(project);
  useEffect(() => {
    if (lastProject.current === project) return;
    lastProject.current = project;
    setSelection(new Set());
    if (doc.camera) {
      cam.current = doc.camera;
      applyCamera(false);
    } else fitAll();
  }, [project, doc.camera, applyCamera, fitAll]);

  useEffect(() => {
    if (!focus) return;
    const it = byId.get(focus.id);
    if (!it) return;
    setSelection(new Set([focus.id]));
    fit(rectOf(it), Math.max(1, cam.current.zoom));
    // Only when asked again, not as the canvas changes.
  }, [focus?.n]);

  // Asked to show what matches: fit it, or say there's none on the canvas.
  const lastFit = useRef(fitMatches);
  useEffect(() => {
    if (fitMatches === lastFit.current) return;
    lastFit.current = fitMatches;
    const hits = refs.filter((it) => !faded?.has(itemId(it)));
    if (hits.length === 0) onToast('Nothing on the canvas matches.');
    else fit(bounds(hits.map(rectOf)));
  }, [fitMatches, refs, faded, fit, onToast]);

  /* ---- The GPU layer's content ---- */

  // The GPU's colors, read from the design tokens once the theme has landed on the page (the
  // window applies it after this view renders), and again when macOS changes appearance.
  const [liveColors, setLiveColors] = useState(readColors);
  useEffect(() => {
    const again = () => requestAnimationFrame(() => setLiveColors(readColors()));
    again();
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    mq.addEventListener('change', again);
    return () => mq.removeEventListener('change', again);
  }, [theme]);

  useEffect(() => {
    const g = gl.current;
    if (!g) return;
    g.gridColor = liveColors.grid;
    const blocks: Block[] = refs.map((it) => {
      const r = rectOf(it);
      const d = data.get(itemId(it));
      let color = liveColors.gone;
      if (d?.ref === 'session') color = liveColors[d.session.status === 'stopped' ? 'idle' : d.session.status];
      // Zoomed out too, a worktree is its column's dark head; its sessions carry the status.
      else if (d?.ref === 'worktree') color = liveColors.lane;
      else if (d?.ref === 'todo') color = d.todo.status === 'done' ? liveColors.done : d.todo.status === 'in-progress' ? liveColors.progress : liveColors.todo;
      else if (d?.ref === 'pr') color = d.pr.state === 'merged' ? liveColors.merged : d.pr.state === 'closed' ? liveColors.gone : liveColors.open;
      return { ...r, color: faded?.has(itemId(it)) ? ([color[0], color[1], color[2], color[3] * 0.18] as Rgba) : color };
    });
    g.setBlocks(blocks);
    g.request(() => cam.current, () => cam.current.zoom < NEAR);
  }, [refs, data, liveColors, faded]);

  useEffect(() => {
    const g = gl.current;
    if (!g) return;
    const from = new Set(selection);
    if (hover) from.add(hover);
    const lines: Line[] = [];
    for (const [a, b] of pairs) {
      if (!from.has(a) && !from.has(b)) continue;
      const ra = byId.get(a);
      const rb = byId.get(b);
      if (!ra || !rb) continue;
      const pa = rectOf(ra);
      const pb = rectOf(rb);
      lines.push({ x1: pa.x + pa.w / 2, y1: pa.y + pa.h / 2, x2: pb.x + pb.w / 2, y2: pb.y + pb.h / 2, color: liveColors.line, width: 1.5 });
    }
    g.setLines(lines);
    g.request(() => cam.current, () => cam.current.zoom < NEAR);
  }, [pairs, selection, hover, byId, liveColors]);

  /* ---- Hit testing ---- */

  const toWorld = useCallback((clientX: number, clientY: number) => {
    const box = rootRef.current?.getBoundingClientRect();
    const c = cam.current;
    const sx = clientX - (box?.left ?? 0);
    const sy = clientY - (box?.top ?? 0);
    return { sx, sy, x: (sx - c.x) / c.zoom, y: (sy - c.y) / c.zoom };
  }, []);

  /** The card or note under a point, topmost first. Frames are picked by their titles. */
  const hitAt = useCallback(
    (x: number, y: number): CanvasItem | null => {
      for (let i = items.length - 1; i >= 0; i--) {
        const it = items[i];
        if (it.kind === 'frame') continue;
        const r = rectOf(it);
        if (x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h) return it;
      }
      return null;
    },
    [items],
  );

  /** Notes are drawn above cards, so a note wins where they overlap. */
  const pick = useCallback(
    (x: number, y: number): CanvasItem | null => {
      const note = [...notes].reverse().find((n) => x >= n.x && x <= n.x + n.w && y >= n.y && y <= n.y + n.h);
      return note ?? hitAt(x, y);
    },
    [notes, hitAt],
  );

  /** What moves with the selection: the selected things, and whatever sits in a selected frame. */
  const carried = useCallback(
    (ids: Set<string>) => {
      const out = new Set(ids);
      for (const id of ids) {
        const f = byId.get(id);
        if (f?.kind !== 'frame') continue;
        for (const it of items) if (it !== f && contains(rectOf(f), rectOf(it))) out.add(itemId(it));
      }
      return out;
    },
    [byId, items],
  );

  /* ---- Changes ---- */

  const removeSelected = useCallback(() => {
    if (selection.size === 0) return;
    const before = doc;
    const gone = [...selection].map((id) => byId.get(id)).filter(Boolean) as CanvasItem[];
    update((d) => ({
      ...d,
      items: d.items.filter((it) => !selection.has(itemId(it))),
      hidden: [...new Set([...d.hidden, ...gone.filter((it) => it.kind === 'ref').map(itemId)])],
    }));
    setSelection(new Set());
    const one = gone.length === 1 ? gone[0] : null;
    const message =
      one?.kind === 'note'
        ? 'Deleted the note.'
        : one?.kind === 'frame'
          ? `Deleted the frame “${one.title}”.`
          : one?.kind === 'ref'
            ? `Took “${titleOf(entities, one.ref, one.key) ?? one.title ?? 'it'}” off the canvas.`
            : `Took ${gone.length} things off the canvas.`;
    onUndo(message, () => update(() => before));
  }, [selection, doc, byId, update, entities, onUndo]);

  const [tidyOpen, setTidyOpen] = useState(false);
  const runTidy = useCallback(
    (scope: Scope, plan: TidyPlan) => {
      const before = doc;
      update((d) => tidy(d, entities, Date.now(), pinned, scope));
      onUndo(`Tidied. ${tidySummary(plan).replace(/^Nothing.*$/, '')}`.trim(), () => update(() => before), 'layout-grid');
      setTidyOpen(false);
      fitted.current = false;
    },
    [doc, update, entities, pinned, onUndo],
  );
  const clearCanvas = useCallback(() => {
    const before = doc;
    update((d) => clearDoc(d, Date.now()));
    setSelection(new Set());
    setTidyOpen(false);
    onUndo('Cleared the canvas. Work comes back as it moves on.', () => update(() => before), 'trash');
  }, [doc, update, onUndo]);

  // After a tidy, the whole canvas comes into view.
  useEffect(() => {
    if (!fitted.current && fitAll()) fitted.current = true;
  }, [items, fitAll]);

  const addNote = useCallback(
    (x: number, y: number) => {
      const note: NoteItem = { kind: 'note', id: newId('note'), text: '', x: Math.round(x), y: Math.round(y), w: 220, h: 140, color: 'gray' };
      update((d) => ({ ...d, items: [...d.items, note] }));
      setSelection(new Set([note.id]));
      setEditing(note.id);
    },
    [update],
  );

  const addFrame = useCallback(
    (r: Rect) => {
      const frame: FrameItem = { kind: 'frame', id: newId('frame'), title: 'Frame', x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.w), h: Math.round(r.h), color: 'gray' };
      // Frames go under everything, so what's in them stays on top.
      update((d) => ({ ...d, items: [frame, ...d.items] }));
      setSelection(new Set([frame.id]));
      setEditing(frame.id);
    },
    [update],
  );

  const patch = useCallback(
    (id: string, fields: Partial<NoteItem> | Partial<FrameItem>) =>
      update((d) => ({ ...d, items: d.items.map((it) => (itemId(it) === id ? ({ ...it, ...fields } as CanvasItem) : it)) })),
    [update],
  );

  const addHere = useCallback(
    (ref: RefKind, key: string, at: { x: number; y: number }) => {
      update((d) => addRef(d, entities, ref, key, at, true));
      setSelection(new Set([refId(ref, key)]));
    },
    [update, entities],
  );

  /** "Add to canvas": recent things of each kind that aren't on it, newest first. */
  const addItems = useCallback(
    (at: { x: number; y: number }): MenuSpec => {
      const on = new Set(refs.map(itemId));
      const pickFrom = <T,>(list: T[], id: (t: T) => string, label: (t: T) => string, when: (t: T) => number, ref: RefKind, key: (t: T) => string) =>
        list
          .filter((t) => !on.has(id(t)))
          .sort((a, b) => when(b) - when(a))
          .slice(0, 20)
          .map((t) => ({ label: label(t).slice(0, 72), run: () => addHere(ref, key(t), at) }));
      return [
        { label: 'Worktree', items: pickFrom([...entities.lanes.values()], (l) => refId('worktree', l.id), (l) => (l.branch && l.branch !== l.callsign ? `${l.callsign} · ${l.branch}` : l.callsign), (l) => l.lastActivity, 'worktree', (l) => l.id) },
        { label: 'Session', items: pickFrom([...entities.sessions.values()], (p) => refId('session', p.session.key), (p) => `${p.session.account}: ${p.session.title} (${p.lane.callsign})`, (p) => p.session.lastActivity, 'session', (p) => p.session.key) },
        { label: 'Todo', items: pickFrom([...entities.todos.values()], (t) => refId('todo', t.id), (t) => t.title || 'Untitled', (t) => t.updatedAt, 'todo', (t) => t.id) },
        { label: 'Pull request', items: pickFrom([...entities.prs.values()], (p) => refId('pr', p.key), (p) => `#${p.number} ${p.title}`, (p) => p.updatedAt, 'pr', (p) => p.key) },
      ];
    },
    [refs, entities, addHere],
  );

  // A note that isn't colored is a yellow sticky.
  const colorItems = (it: NoteItem | FrameItem): MenuSpec =>
    LABEL_COLORS.map((c: LabelColor) => ({ label: c === 'gray' && it.kind === 'note' ? 'Yellow' : COLOR_WORD[c], checked: it.color === c, run: () => patch(it.id, { color: c }) }));

  /* ---- Pointer ---- */

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    const target = e.target as HTMLElement;
    if (target.closest('.ca-cv-toolbar, .ca-cv-popover, .ca-cv-tidy, textarea, input')) return;
    setTidyOpen(false);
    if (e.button === 2) return;
    rootRef.current?.focus({ preventScroll: true });
    const p = toWorld(e.clientX, e.clientY);
    window.clearTimeout(hoverTimer.current);
    setPopover(null);
    if (editing) setEditing(null);
    const capture = () => rootRef.current?.setPointerCapture(e.pointerId);

    if (e.button === 1 || spaceHeld) {
      drag.current = { mode: 'pan', sx: e.clientX, sy: e.clientY, cam: { ...cam.current } };
      capture();
      e.preventDefault();
      return;
    }
    if (e.button !== 0) return;
    if (tool === 'note') {
      addNote(p.x, p.y);
      setTool('select');
      return;
    }
    if (tool === 'frame') {
      drag.current = { mode: 'frame', sx: p.x, sy: p.y };
      setMarquee({ x: p.x, y: p.y, w: 0, h: 0 });
      capture();
      return;
    }
    const handle = target.closest<HTMLElement>('[data-resize]');
    if (handle) {
      const it = byId.get(handle.dataset.resize ?? '');
      if (it && it.kind !== 'ref') {
        drag.current = { mode: 'resize', id: it.id, sx: p.x, sy: p.y, w: it.w, h: it.h };
        capture();
        return;
      }
    }
    const frameTitle = target.closest<HTMLElement>('[data-frame]');
    const hit = frameTitle ? (byId.get(frameTitle.dataset.frame ?? '') ?? null) : pick(p.x, p.y);
    if (hit) {
      const id = itemId(hit);
      let next = selection;
      let toggle: string | null = null;
      if (e.shiftKey) {
        if (selection.has(id)) toggle = id;
        else next = new Set([...selection, id]);
      } else if (!selection.has(id)) next = new Set([id]);
      if (next !== selection) setSelection(next);
      const start = new Map<string, { x: number; y: number }>();
      for (const mid of carried(next)) {
        const it = byId.get(mid);
        if (it) start.set(mid, { x: it.x, y: it.y });
      }
      drag.current = { mode: 'move', sx: p.x, sy: p.y, moved: false, start, toggle };
      capture();
      return;
    }
    drag.current = { mode: 'marquee', sx: p.x, sy: p.y, add: e.shiftKey ? new Set(selection) : new Set() };
    if (!e.shiftKey) setSelection(new Set());
    setMarquee({ x: p.x, y: p.y, w: 0, h: 0 });
    capture();
  };

  const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    const p = toWorld(e.clientX, e.clientY);
    if (!d) {
      // Hover: the card under the pointer, and after a moment its popover.
      const target = e.target as HTMLElement;
      const over = target.closest('.ca-cv-toolbar') ? null : hitAt(p.x, p.y);
      const id = over?.kind === 'ref' ? itemId(over) : null;
      if (id !== hover) {
        setHover(id);
        setPopover(null);
        window.clearTimeout(hoverTimer.current);
        if (id) hoverTimer.current = window.setTimeout(() => setPopover(id), HOVER_MS);
      }
      return;
    }
    switch (d.mode) {
      case 'pan':
        cam.current = { ...d.cam, x: d.cam.x + e.clientX - d.sx, y: d.cam.y + e.clientY - d.sy };
        applyCamera();
        return;
      case 'move': {
        const dx = p.x - d.sx;
        const dy = p.y - d.sy;
        if (!d.moved && Math.hypot(dx, dy) * cam.current.zoom < 3) return;
        d.moved = true;
        setHover(null);
        update((doc) => ({
          ...doc,
          items: doc.items.map((it) => {
            const s = d.start.get(itemId(it));
            return s ? { ...it, x: Math.round(s.x + dx), y: Math.round(s.y + dy) } : it;
          }),
        }));
        return;
      }
      case 'marquee':
      case 'frame': {
        const r = { x: Math.min(d.sx, p.x), y: Math.min(d.sy, p.y), w: Math.abs(p.x - d.sx), h: Math.abs(p.y - d.sy) };
        setMarquee(r);
        if (d.mode === 'marquee') {
          const hitIds = items.filter((it) => (it.kind === 'frame' ? contains(r, it) && r.w >= it.w * 0.9 : overlaps(r, rectOf(it)))).map(itemId);
          setSelection(new Set([...d.add, ...hitIds]));
        }
        return;
      }
      case 'resize':
        patch(d.id, { w: Math.max(120, Math.round(d.w + p.x - d.sx)), h: Math.max(60, Math.round(d.h + p.y - d.sy)) });
        return;
    }
  };

  const onPointerUp = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    drag.current = null;
    rootRef.current?.releasePointerCapture?.(e.pointerId);
    if (!d) return;
    if (d.mode === 'move') {
      if (!d.moved && d.toggle) {
        const next = new Set(selection);
        next.delete(d.toggle);
        setSelection(next);
      }
      // Moved cards are yours: they stay when they're no longer recent.
      if (d.moved) update((doc) => ({ ...doc, items: doc.items.map((it) => (it.kind === 'ref' && d.start.has(itemId(it)) && !it.kept ? { ...it, kept: true } : it)) }));
    }
    if (d.mode === 'marquee') setMarquee(null);
    if (d.mode === 'frame') {
      setMarquee(null);
      const p = toWorld(e.clientX, e.clientY);
      const r = { x: Math.min(d.sx, p.x), y: Math.min(d.sy, p.y), w: Math.abs(p.x - d.sx), h: Math.abs(p.y - d.sy) };
      addFrame(r.w * cam.current.zoom < 8 ? { x: d.sx, y: d.sy, w: 560, h: 360 } : r);
      setTool('select');
    }
  };

  const onDoubleClick = (e: ReactMouseEvent) => {
    const target = e.target as HTMLElement;
    if (target.closest('.ca-cv-toolbar, .ca-cv-tidy, textarea, input')) return;
    const frameTitle = target.closest<HTMLElement>('[data-frame]');
    if (frameTitle) {
      setEditing(frameTitle.dataset.frame ?? null);
      return;
    }
    const p = toWorld(e.clientX, e.clientY);
    const hit = pick(p.x, p.y);
    if (hit?.kind === 'ref') onOpen(hit);
    else if (hit?.kind === 'note') setEditing(hit.id);
  };

  const onContextMenu = (e: ReactMouseEvent) => {
    e.preventDefault();
    const target = e.target as HTMLElement;
    if (target.closest('.ca-cv-toolbar, .ca-cv-tidy, textarea, input')) return;
    const p = toWorld(e.clientX, e.clientY);
    const frameTitle = target.closest<HTMLElement>('[data-frame]');
    const hit = frameTitle ? (byId.get(frameTitle.dataset.frame ?? '') ?? null) : pick(p.x, p.y);
    const show = (spec: MenuSpec) => void showContextMenu(spec).catch((err: unknown) => onToast(String(err)));
    setPopover(null);
    if (hit) {
      const id = itemId(hit);
      if (!selection.has(id)) setSelection(new Set([id]));
      const many = selection.has(id) && selection.size > 1;
      const remove = { label: many ? `Take ${selection.size} off the canvas` : hit.kind === 'ref' ? 'Take off the canvas' : hit.kind === 'note' ? 'Delete note' : 'Delete frame', run: () => (many ? removeSelected() : removeOne(id)) };
      if (hit.kind === 'ref') show([...(many ? [] : menuFor(hit)), 'separator', remove]);
      else if (hit.kind === 'note') show([{ label: 'Edit', run: () => setEditing(hit.id) }, { label: 'Color', items: colorItems(hit) }, 'separator', remove]);
      else show([{ label: 'Rename…', run: () => setEditing(hit.id) }, { label: 'Color', items: colorItems(hit) }, 'separator', remove]);
      return;
    }
    show([
      { label: 'Add note here', run: () => addNote(p.x, p.y) },
      { label: 'Add frame here', run: () => addFrame({ x: p.x, y: p.y, w: 560, h: 360 }) },
      { label: 'Add to canvas', items: addItems({ x: p.x, y: p.y }) },
      'separator',
      { label: 'Tidy up', run: () => runTidy(doc.scope, tidyPlan(doc, entities, now, pinned, doc.scope)) },
      { label: 'Tidy…', run: () => setTidyOpen(true) },
      { label: 'Clear canvas', run: clearCanvas },
      'separator',
      { label: 'Zoom to fit', run: fitAll },
    ]);
  };

  // Deleting one thing from its menu, without disturbing the rest of the selection.
  const removeOneRef = useRef<(id: string) => void>(() => {});
  removeOneRef.current = (id: string) => {
    const before = doc;
    const it = byId.get(id);
    update((d) => ({ ...d, items: d.items.filter((x) => itemId(x) !== id), hidden: it?.kind === 'ref' ? [...new Set([...d.hidden, id])] : d.hidden }));
    setSelection((s) => {
      const n = new Set(s);
      n.delete(id);
      return n;
    });
    onUndo(it?.kind === 'note' ? 'Deleted the note.' : it?.kind === 'frame' ? `Deleted the frame “${it.title}”.` : 'Took it off the canvas.', () => update(() => before));
  };
  const removeOne = (id: string) => removeOneRef.current(id);

  // Wheel and pinch: listened to directly, since React's wheel listener can't prevent the page's own.
  useEffect(() => {
    const el = rootRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if ((e.target as HTMLElement).closest('.ca-cv-popover, textarea')) return;
      e.preventDefault();
      const box = el.getBoundingClientRect();
      const line = e.deltaMode === 1 ? 16 : 1;
      if (e.ctrlKey || e.metaKey) {
        if (gestureZoom.current) return;
        const z = cam.current.zoom * Math.exp(-e.deltaY * line * (e.ctrlKey ? 0.01 : 0.0025));
        zoomAt(e.clientX - box.left, e.clientY - box.top, z);
      } else {
        cam.current = { ...cam.current, x: cam.current.x - e.deltaX * line, y: cam.current.y - e.deltaY * line };
        applyCamera();
      }
    };
    // WebKit's pinch comes as gesture events.
    type Gesture = Event & { scale: number; clientX: number; clientY: number };
    const onGestureStart = (e: Event) => {
      e.preventDefault();
      const g = e as Gesture;
      const box = el.getBoundingClientRect();
      gestureZoom.current = { zoom: cam.current.zoom, x: g.clientX - box.left, y: g.clientY - box.top };
    };
    const onGestureChange = (e: Event) => {
      e.preventDefault();
      const g = gestureZoom.current;
      if (g) zoomAt(g.x, g.y, g.zoom * (e as Gesture).scale);
    };
    const onGestureEnd = (e: Event) => {
      e.preventDefault();
      gestureZoom.current = null;
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    el.addEventListener('gesturestart', onGestureStart);
    el.addEventListener('gesturechange', onGestureChange);
    el.addEventListener('gestureend', onGestureEnd);
    return () => {
      el.removeEventListener('wheel', onWheel);
      el.removeEventListener('gesturestart', onGestureStart);
      el.removeEventListener('gesturechange', onGestureChange);
      el.removeEventListener('gestureend', onGestureEnd);
    };
  }, [zoomAt, applyCamera]);

  /* ---- Keys ---- */

  const zoomBy = useCallback(
    (f: number) => {
      const el = rootRef.current;
      zoomAt((el?.clientWidth ?? 0) / 2, (el?.clientHeight ?? 0) / 2, cam.current.zoom * f);
    },
    [zoomAt],
  );
  const zoomTo = useCallback(
    (z: number) => {
      const el = rootRef.current;
      zoomAt((el?.clientWidth ?? 0) / 2, (el?.clientHeight ?? 0) / 2, z);
    },
    [zoomAt],
  );

  keys.current = (e: KeyboardEvent) => {
    const key = e.key.toLowerCase();
    const mod = e.metaKey || e.ctrlKey;
    if (e.key === ' ') {
      e.preventDefault();
      if (!e.repeat) setSpaceHeld(true);
      return true;
    }
    if (tidyOpen && e.key === 'Escape') {
      setTidyOpen(false);
      return true;
    }
    // Return presses the panel's focused button.
    if (tidyOpen && e.key === 'Enter') return false;
    if (e.key === 'Escape') {
      if (tool !== 'select') setTool('select');
      else setSelection(new Set());
      return true;
    }
    if (e.shiftKey && !mod && (e.code === 'Digit1' || e.code === 'Digit2' || e.code === 'Digit0')) {
      e.preventDefault();
      if (e.code === 'Digit1') fitAll();
      else if (e.code === 'Digit2') fitSelection();
      else zoomTo(1);
      return true;
    }
    if (mod && key === '0') {
      e.preventDefault();
      zoomTo(1);
      return true;
    }
    if ((mod && (key === '=' || key === '+')) || (!mod && (e.key === '=' || e.key === '+'))) {
      e.preventDefault();
      zoomBy(1.25);
      return true;
    }
    if ((mod && key === '-') || (!mod && e.key === '-')) {
      e.preventDefault();
      zoomBy(0.8);
      return true;
    }
    if (mod && key === 'a') {
      e.preventDefault();
      setSelection(new Set(items.map(itemId)));
      return true;
    }
    if (mod || e.altKey) return false;
    if (key === 'n') {
      setTool('note');
      return true;
    }
    if (key === 'f' || key === 'r') {
      setTool('frame');
      return true;
    }
    if (e.key === 'Backspace' || e.key === 'Delete') {
      e.preventDefault();
      removeSelected();
      return true;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      const only = selection.size === 1 ? byId.get([...selection][0]) : undefined;
      if (only?.kind === 'ref') onOpen(only);
      else if (only) setEditing(itemId(only));
      return true;
    }
    if (e.key.startsWith('Arrow')) {
      e.preventDefault();
      const step = e.shiftKey ? 10 : 1;
      const dx = e.key === 'ArrowLeft' ? -step : e.key === 'ArrowRight' ? step : 0;
      const dy = e.key === 'ArrowUp' ? -step : e.key === 'ArrowDown' ? step : 0;
      if (selection.size === 0) {
        cam.current = { ...cam.current, x: cam.current.x - dx * 20, y: cam.current.y - dy * 20 };
        applyCamera();
        return true;
      }
      const moving = carried(selection);
      update((d) => ({ ...d, items: d.items.map((it) => (moving.has(itemId(it)) ? { ...it, x: it.x + dx, y: it.y + dy, ...(it.kind === 'ref' ? { kept: true } : {}) } : it)) }));
      return true;
    }
    // J and K walk the list in other views; here there's no list.
    return key === 'j' || key === 'k';
  };
  useEffect(() => {
    const up = (e: KeyboardEvent) => {
      if (e.key === ' ') setSpaceHeld(false);
    };
    const blur = () => setSpaceHeld(false);
    window.addEventListener('keyup', up);
    window.addEventListener('blur', blur);
    return () => {
      window.removeEventListener('keyup', up);
      window.removeEventListener('blur', blur);
    };
  }, []);

  // The window shows the selected card's panel. Only a change of card counts, not its moving.
  const selectedRef = selection.size === 1 ? byId.get([...selection][0]) : undefined;
  const selectedId = selectedRef?.kind === 'ref' ? itemId(selectedRef) : null;
  useEffect(() => {
    onSelectItem(selectedRef?.kind === 'ref' ? selectedRef : null);
    // selectedRef changes as the card moves; the id is what matters.
  }, [selectedId, onSelectItem]);
  useEffect(() => () => onSelectItem(null), [onSelectItem]);

  /* ---- Drawing ---- */

  const shownCards = useMemo(() => (near || glError ? refs.filter((it) => overlaps(cull, rectOf(it))) : []), [near, glError, refs, cull]);
  // Zoomed out, worktrees keep their names, drawn at a size you can read.
  const farLabels = useMemo(() => (!near && farNames && !glError ? refs.filter((it) => it.ref === 'worktree' && overlaps(cull, rectOf(it))) : []), [near, farNames, glError, refs, cull]);
  const single = selection.size === 1 ? byId.get([...selection][0]) : undefined;
  const popItem = popover ? byId.get(popover) : undefined;
  const popRect = useMemo(() => {
    if (!popItem || popItem.kind !== 'ref') return null;
    const r = rectOf(popItem);
    const c = cam.current;
    const el = rootRef.current;
    const w = el?.clientWidth ?? 0;
    const h = el?.clientHeight ?? 0;
    const left = r.x * c.zoom + c.x;
    const right = (r.x + r.w) * c.zoom + c.x;
    const top = r.y * c.zoom + c.y;
    const width = 340;
    const x = right + 10 + width <= w - 8 ? right + 10 : Math.max(8, left - 10 - width);
    return { left: x, top: Math.min(Math.max(8, top), Math.max(8, h - 280)) };
    // The camera is a ref; the popover closes whenever it moves.
  }, [popItem]);

  return (
    <div
      ref={rootRef}
      className={cx('ca-cv', moving && 'is-moving', spaceHeld && 'is-panning', tool !== 'select' && `is-tool-${tool}`, near ? 'is-near' : 'is-far')}
      tabIndex={-1}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      onPointerLeave={() => {
        if (drag.current) return;
        window.clearTimeout(hoverTimer.current);
        setHover(null);
        setPopover(null);
      }}
      onDoubleClick={onDoubleClick}
      onContextMenu={onContextMenu}
      aria-label="Canvas"
    >
      <canvas ref={glCanvas} className="ca-cv-gl" aria-hidden="true" />
      <div ref={worldRef} className="ca-cv-world">
        {frames.map((f) => (
          <div key={f.id} className={cx('ca-cv-frame', `is-${f.color}`, selection.has(f.id) && 'is-selected')} style={{ transform: `translate(${f.x}px, ${f.y}px)`, width: f.w, height: f.h }}>
            <div className="ca-cv-frame-title" data-frame={f.id}>
              {editing === f.id ? (
                <input
                  autoFocus
                  defaultValue={f.title}
                  onFocus={(e) => e.currentTarget.select()}
                  onBlur={(e) => {
                    const title = e.currentTarget.value.trim() || 'Frame';
                    if (title !== f.title) patch(f.id, { title, auto: false });
                    setEditing(null);
                  }}
                  onKeyDown={(e) => {
                    e.stopPropagation();
                    if (e.key === 'Enter' || e.key === 'Escape') e.currentTarget.blur();
                  }}
                  aria-label="Frame name"
                  spellCheck={false}
                />
              ) : (
                f.title
              )}
            </div>
            {single === f && <div className="ca-cv-resize" data-resize={f.id} />}
          </div>
        ))}
        {farLabels.map((it) => (
          <div key={itemId(it)} className={cx('ca-cv-farlabel', faded?.has(itemId(it)) && 'is-faded')} style={{ transform: `translate(${it.x}px, ${it.y}px) scale(calc(1 / var(--z, 1)))` }}>
            {titleOf(entities, it.ref, it.key) ?? it.title}
          </div>
        ))}
        {shownCards.map((it) => {
          const id = itemId(it);
          const d = data.get(id) ?? null;
          return <CanvasCard key={id} item={it} data={d} sig={cardSig(d, now)} now={now} selected={selection.has(id)} related={related.has(id)} faded={!!faded?.has(id)} />;
        })}
        {notes.map((n) => (
          <div key={n.id} className={cx('ca-cv-note', `is-${n.color}`, selection.has(n.id) && 'is-selected', faded?.has(n.id) && 'is-faded')} style={{ transform: `translate(${n.x}px, ${n.y}px)`, width: n.w, height: n.h }}>
            {editing === n.id ? (
              <textarea
                autoFocus
                defaultValue={n.text}
                onBlur={(e) => {
                  if (e.currentTarget.value !== n.text) patch(n.id, { text: e.currentTarget.value });
                  setEditing(null);
                }}
                onKeyDown={(e) => {
                  e.stopPropagation();
                  if (e.key === 'Escape' || (e.key === 'Enter' && e.metaKey)) e.currentTarget.blur();
                }}
                aria-label="Note"
                placeholder="Write a note"
              />
            ) : (
              <div className={cx('ca-cv-note-text', !n.text && 'is-empty')}>{n.text || 'Double-click to write'}</div>
            )}
            {single === n && <div className="ca-cv-resize" data-resize={n.id} />}
          </div>
        ))}
        {marquee && <div className={cx('ca-cv-marquee', tool === 'frame' && 'is-frame')} style={{ transform: `translate(${marquee.x}px, ${marquee.y}px)`, width: marquee.w, height: marquee.h }} />}
      </div>
      {items.length === 0 && (
        <div className="ca-cv-empty">
          <p>
            {doc.scope.clearedAt != null
              ? 'Cleared. Sessions and todos come on here as they move on; right-click to add one now.'
              : `Nothing recent in ${project}. Right-click to add a worktree, a session or a note.`}
          </p>
        </div>
      )}
      {popItem?.kind === 'ref' && popRect && (
        <div className="ca-cv-popover" style={popRect} role="tooltip">
          <CardDetail item={popItem} data={data.get(itemId(popItem)) ?? null} now={now} terminal={terminal} todos={todos} />
        </div>
      )}
      {tidyOpen && <TidyPanel doc={doc} entities={entities} now={now} pinned={pinned} onTidy={runTidy} onClear={clearCanvas} onClose={() => setTidyOpen(false)} />}
      <div className="ca-cv-toolbar" role="toolbar" aria-label="Canvas tools" onDoubleClick={(e) => e.stopPropagation()}>
        <button type="button" className={cx('ca-cv-tool', tool === 'note' && 'is-on')} title="Note (N): click where it goes" aria-pressed={tool === 'note'} onClick={() => setTool(tool === 'note' ? 'select' : 'note')}>
          <Icon name="note" size={16} />
        </button>
        <button type="button" className={cx('ca-cv-tool', tool === 'frame' && 'is-on')} title="Frame (F): drag to draw one" aria-pressed={tool === 'frame'} onClick={() => setTool(tool === 'frame' ? 'select' : 'frame')}>
          <Icon name="frame" size={16} />
        </button>
        <span className="ca-cv-sep" />
        <button
          type="button"
          className="ca-cv-tool is-wide"
          title="Lay the canvas out again, and choose how far back its sessions and todos go"
          aria-expanded={tidyOpen}
          onClick={() => setTidyOpen(!tidyOpen)}
        >
          <Icon name="layout-grid" size={15} />
          <span>Tidy</span>
          <Icon name="chevron-down" size={12} />
        </button>
        <button
          type="button"
          className="ca-cv-tool is-wide"
          title="Add a worktree, session, todo or PR"
          onClick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            const v = viewport();
            void showContextMenu(addItems({ x: v.x + v.w / 2 - CARD_W / 2, y: v.y + v.h / 2 }), { x: r.left, y: r.top - 110 }).catch((err: unknown) => onToast(String(err)));
          }}
        >
          <Icon name="plus" size={15} />
          <span>Add</span>
        </button>
        <span className="ca-cv-sep" />
        <button type="button" className="ca-cv-tool" title="Zoom out (−)" onClick={() => zoomBy(0.8)}>
          <Icon name="zoom-out" size={16} />
        </button>
        <button ref={zoomLabel} type="button" className="ca-cv-zoom" title="Zoom to fit (⇧1). ⇧2 zooms to the selection, ⇧0 to 100%." onClick={fitAll}>
          {Math.round(cam.current.zoom * 100)}%
        </button>
        <button type="button" className="ca-cv-tool" title="Zoom in (+)" onClick={() => zoomBy(1.25)}>
          <Icon name="zoom-in" size={16} />
        </button>
      </div>
      {glError && <div className="ca-cv-glerror">{glError}</div>}
    </div>
  );
}
