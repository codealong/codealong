// Dynamic Working Set mode for Canvas: a self-curating spatial map of live sessions,
// schedule hubs and auto-clustered workstreams with damped force layout, drag-to-group/detach,
// one-key shelving, action spotlight and Tab-to-next-action navigation.

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type MutableRefObject,
  type PointerEvent as ReactPointerEvent,
} from 'react';
import { takesAll, type BoardDoc } from '../boards/model';
import { showContextMenu, type MenuSpec } from '../contextMenu';
import { ago, duration } from '../format';
import { arrowhead, connector, pathOf } from '../lineage';
import { chordMod, keysIn, pinch, zoomWheel } from '../platform';
import { useStored, type Codec } from '../stored';
import type { Label, PrItem, ScheduleItem, Todo } from '../types';
import { cleanMapState, refId, type Camera, type CanvasDoc, type Entities, type MapLayoutState, type OnBoard, type RefItem } from '../canvas/model';
import {
  buildWorkingSet,
  detachNode,
  emptyPrefs,
  groupNodes,
  pinClusterMoved,
  pinNodeAt,
  pullInNode,
  relaxLayout,
  renameCluster,
  shelveNode,
  type NodeBox,
  type WorkingCluster,
  type WorkingEdge,
  type WorkingNode,
  type WorkingSetPrefs,
} from '../canvas/workingSet';
import { cx, Icon, PrChip, SessionEmblem, SessionGlyph } from './primitives';
import { WorkingSetClusterPanel } from './WorkingSetClusterPanel';

const MIN_ZOOM = 0.25;
const MAX_ZOOM = 2.2;
const clampZoom = (z: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));

const PREFS_BY_PROJECT: Codec<Record<string, WorkingSetPrefs>> = {
  parse: (raw) => {
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return undefined;
    const out: Record<string, WorkingSetPrefs> = {};
    for (const [k, v] of Object.entries(parsed)) {
      if (!v || typeof v !== 'object') continue;
      const p = v as Partial<WorkingSetPrefs>;
      out[k] = {
        pinnedPos: p.pinnedPos && typeof p.pinnedPos === 'object' ? p.pinnedPos : {},
        shelved: Array.isArray(p.shelved) ? p.shelved.filter((x): x is string => typeof x === 'string') : [],
        pinnedIn: Array.isArray(p.pinnedIn) ? p.pinnedIn.filter((x): x is string => typeof x === 'string') : [],
        groups: p.groups && typeof p.groups === 'object' ? p.groups : {},
        groupNames: p.groupNames && typeof p.groupNames === 'object' ? p.groupNames : {},
      };
    }
    return out;
  },
  format: (value) => JSON.stringify(value),
};

const EMPTY_MAP: Record<string, WorkingSetPrefs> = {};

function mapToPrefs(map: MapLayoutState | undefined): WorkingSetPrefs {
  return {
    pinnedPos: map?.pinnedPos ?? {},
    groups: map?.groups ?? {},
    groupNames: map?.groupNames ?? {},
    shelved: map?.shelvedHubs ?? [],
    pinnedIn: [],
  };
}

function prefsToMap(p: WorkingSetPrefs): MapLayoutState | undefined {
  return cleanMapState({
    pinnedPos: p.pinnedPos,
    groups: p.groups,
    groupNames: p.groupNames,
    shelvedHubs: p.shelved.filter((id) => id.startsWith('schedule:')),
  });
}

type DragState =
  | { kind: 'pan'; sx: number; sy: number; cam: Camera }
  | { kind: 'node'; id: string; sx: number; sy: number; ox: number; oy: number; clusterId: string | null; moved: boolean }
  | { kind: 'cluster'; id: string; sx: number; sy: number; nodeIds: string[]; moved: boolean };

export interface WorkingSetCanvasProps {
  /** When backed by a Board, membership comes from the board and Map overrides persist in `doc.map`. */
  board?: BoardDoc | null;
  doc?: CanvasDoc | null;
  update?: (fn: (doc: CanvasDoc) => CanvasDoc) => void;
  holds?: OnBoard;
  onTakeOff?: (cards: string[]) => void;
  onPutOn?: (cards: string[]) => void;
  onSettings?: () => void;
  entities: Entities;
  schedules: ScheduleItem[];
  prs: PrItem[];
  todos: Todo[];
  labels: Map<string, Label>;
  now: number;
  project: string;
  query: string;
  filtering?: boolean;
  matches?: (it: RefItem) => boolean;
  onSelectItem: (it: RefItem | null) => void;
  onOpen: (it: RefItem) => void;
  onShowSchedule?: (id: string) => void;
  menuFor: (it: RefItem) => MenuSpec;
  onToast: (message: string) => void;
  onUndo: (message: string, undo: () => void, icon?: string) => void;
  keys: MutableRefObject<(e: KeyboardEvent) => boolean>;
}

function edgeStrokeClass(e: WorkingEdge, active: boolean): string {
  return cx('ca-ws-edge', `is-${e.kind}`, e.crossCluster && 'is-cross', active && 'is-active');
}

function boxCenter(b: NodeBox) {
  return { x: b.x + b.w / 2, y: b.y + b.h / 2 };
}

function hitsBox(pt: { x: number; y: number }, b: NodeBox, pad = 0): boolean {
  return pt.x >= b.x - pad && pt.x <= b.x + b.w + pad && pt.y >= b.y - pad && pt.y <= b.y + b.h + pad;
}

export function WorkingSetCanvas({
  board = null,
  doc = null,
  update,
  holds,
  onTakeOff,
  onPutOn,
  onSettings,
  entities,
  schedules,
  prs,
  todos,
  labels,
  now,
  project,
  query,
  filtering = false,
  matches,
  onSelectItem,
  onOpen,
  onShowSchedule,
  menuFor,
  onToast,
  onUndo,
  keys,
}: WorkingSetCanvasProps) {
  const [localPrefs, setLocalPrefs] = useStored<Record<string, WorkingSetPrefs>>('ca-ws-prefs', PREFS_BY_PROJECT, EMPTY_MAP);
  const boardBacked = !!(board && update);
  const prefs = useMemo(
    () => (boardBacked ? mapToPrefs(doc?.map) : (localPrefs[project] ?? emptyPrefs())),
    [boardBacked, doc?.map, localPrefs, project],
  );

  const updatePrefs = useCallback(
    (fn: (cur: WorkingSetPrefs) => WorkingSetPrefs) => {
      if (boardBacked && update) {
        update((d) => {
          const cur = mapToPrefs(d.map);
          const next = fn(cur);
          if (next === cur) return d;
          const map = prefsToMap(next);
          const copy: CanvasDoc = { ...d };
          if (map) copy.map = map;
          else delete copy.map;
          return copy;
        });
        return;
      }
      setLocalPrefs((prev) => {
        const cur = prev[project] ?? emptyPrefs();
        const next = fn(cur);
        return next === cur ? prev : { ...prev, [project]: next };
      });
    },
    [boardBacked, update, project, setLocalPrefs],
  );

  const boardScope = useMemo(
    () => (board && holds ? { board, member: holds.has, shows: holds.shows } : undefined),
    [board, holds],
  );

  const graph = useMemo(
    () => buildWorkingSet({ entities, schedules, prs, todos, labels, now, prefs, boardScope }),
    [entities, schedules, prs, todos, labels, now, prefs, boardScope],
  );

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [selectedClusterId, setSelectedClusterId] = useState<string | null>(null);
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const [actionOnly, setActionOnly] = useState(false);
  const [editingCluster, setEditingCluster] = useState<string | null>(null);
  const [spaceHeld, setSpaceHeld] = useState(false);
  const [dragDelta, setDragDelta] = useState<{ ids: Set<string>; dx: number; dy: number } | null>(null);
  const [dropTarget, setDropTarget] = useState<{ kind: 'node' | 'cluster'; id: string } | null>(null);
  const [actionIdx, setActionIdx] = useState(0);

  const rootRef = useRef<HTMLDivElement>(null);
  const worldRef = useRef<HTMLDivElement>(null);
  const zoomLabelRef = useRef<HTMLButtonElement>(null);
  const cam = useRef<Camera>({ x: 0, y: 0, zoom: 1 });
  const drag = useRef<DragState | null>(null);
  const fittedProject = useRef<string | null>(null);

  const nodeById = useMemo(() => new Map(graph.nodes.map((n) => [n.id, n])), [graph.nodes]);
  const clusterById = useMemo(() => new Map(graph.clusters.map((c) => [c.id, c])), [graph.clusters]);
  const clusterOfNode = useMemo(() => {
    const m = new Map<string, WorkingCluster>();
    for (const c of graph.clusters) for (const id of c.nodeIds) m.set(id, c);
    return m;
  }, [graph.clusters]);

  // Live boxes adjusted by any active drag delta so edges follow dragged cards smoothly.
  const liveBoxes = useMemo(() => {
    if (!dragDelta) return graph.boxes;
    const out = new Map(graph.boxes);
    for (const id of dragDelta.ids) {
      const b = out.get(id);
      if (b) out.set(id, { ...b, x: b.x + dragDelta.dx, y: b.y + dragDelta.dy });
    }
    return out;
  }, [graph.boxes, dragDelta]);

  const applyCamera = useCallback(() => {
    const c = cam.current;
    if (worldRef.current) {
      worldRef.current.style.transform = `translate(${c.x}px, ${c.y}px) scale(${c.zoom})`;
      worldRef.current.style.setProperty('--z', String(c.zoom));
    }
    if (zoomLabelRef.current) zoomLabelRef.current.textContent = `${Math.round(c.zoom * 100)}%`;
  }, []);

  const fitBounds = useCallback(
    (rects: NodeBox[], maxZ = 1.05) => {
      const el = rootRef.current;
      if (!el || rects.length === 0) return;
      const vw = el.clientWidth;
      const vh = el.clientHeight;
      if (vw <= 0 || vh <= 0) return;
      let minX = Infinity;
      let minY = Infinity;
      let maxX = -Infinity;
      let maxY = -Infinity;
      for (const r of rects) {
        minX = Math.min(minX, r.x);
        minY = Math.min(minY, r.y);
        maxX = Math.max(maxX, r.x + r.w);
        maxY = Math.max(maxY, r.y + r.h);
      }
      const padTop = 76;
      const padSide = 48;
      const padBottom = 78;
      const availW = Math.max(200, vw - padSide * 2);
      const availH = Math.max(180, vh - padTop - padBottom);
      const bw = Math.max(1, maxX - minX);
      const bh = Math.max(1, maxY - minY);
      const z = clampZoom(Math.min(maxZ, availW / bw, availH / bh));
      const cx = (minX + maxX) / 2;
      const cy = (minY + maxY) / 2;
      cam.current = { x: vw / 2 - cx * z, y: padTop + availH / 2 - cy * z, zoom: z };
      applyCamera();
    },
    [applyCamera],
  );

  const fitAll = useCallback(() => {
    const rects: NodeBox[] = [
      ...graph.clusters.map((c) => ({ x: c.x, y: c.y, w: c.w, h: c.h })),
      ...[...graph.boxes.values()],
    ];
    fitBounds(rects, 1.0);
  }, [graph.clusters, graph.boxes, fitBounds]);

  const zoomAt = useCallback(
    (sx: number, sy: number, nextZ: number) => {
      const c = cam.current;
      const z = clampZoom(nextZ);
      const wx = (sx - c.x) / c.zoom;
      const wy = (sy - c.y) / c.zoom;
      cam.current = { x: sx - wx * z, y: sy - wy * z, zoom: z };
      applyCamera();
    },
    [applyCamera],
  );

  const zoomBy = useCallback(
    (factor: number) => {
      const el = rootRef.current;
      zoomAt((el?.clientWidth ?? 0) / 2, (el?.clientHeight ?? 0) / 2, cam.current.zoom * factor);
    },
    [zoomAt],
  );

  const centerOn = useCallback(
    (id: string) => {
      const b = graph.boxes.get(id);
      const el = rootRef.current;
      if (!b || !el) return;
      const z = Math.max(0.85, Math.min(1.05, cam.current.zoom));
      cam.current = { x: el.clientWidth / 2 - (b.x + b.w / 2) * z, y: el.clientHeight / 2 - (b.y + b.h / 2) * z, zoom: z };
      applyCamera();
    },
    [graph.boxes, applyCamera],
  );

  // Auto-fit once per project/board load (and whenever the layout goes from empty to populated).
  const fitKey = `${project}:${board?.id ?? ''}`;
  useLayoutEffect(() => {
    if (graph.nodes.length === 0) return;
    if (fittedProject.current === fitKey) return;
    fittedProject.current = fitKey;
    fitAll();
  }, [fitKey, graph.nodes.length, fitAll]);

  // Clear selection when switching to another board.
  useEffect(() => {
    setSelectedId(null);
    setSelectedClusterId(null);
  }, [board?.id]);

  // Keep MainPanel synchronized with the selected session node.
  useEffect(() => {
    const node = selectedId ? nodeById.get(selectedId) : undefined;
    if (node?.kind === 'session') {
      onSelectItem({ kind: 'ref', ref: 'session', key: node.key, x: 0, y: 0, title: node.session.title });
    } else {
      onSelectItem(null);
    }
  }, [selectedId, nodeById, onSelectItem]);

  useEffect(() => () => onSelectItem(null), [onSelectItem]);

  // Clear stale selection if a node leaves the working set.
  useEffect(() => {
    if (selectedId && !nodeById.has(selectedId)) setSelectedId(null);
    if (selectedClusterId && !clusterById.has(selectedClusterId)) setSelectedClusterId(null);
  }, [selectedId, selectedClusterId, nodeById, clusterById]);

  // Wheel pan & pinch zoom (matching CanvasView).
  useEffect(() => {
    const el = rootRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if ((e.target as HTMLElement).closest('.ca-ws-panel')) return;
      e.preventDefault();
      const line = e.deltaMode === 1 ? 16 : 1;
      if (zoomWheel(e)) {
        const rect = el.getBoundingClientRect();
        const z = cam.current.zoom * Math.exp(-e.deltaY * line * (pinch(e) ? 0.01 : 0.0025));
        zoomAt(e.clientX - rect.left, e.clientY - rect.top, z);
        return;
      }
      cam.current = { ...cam.current, x: cam.current.x - e.deltaX * line, y: cam.current.y - e.deltaY * line };
      applyCamera();
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [zoomAt, applyCamera]);

  const toWorld = useCallback((clientX: number, clientY: number) => {
    const rect = rootRef.current?.getBoundingClientRect();
    const sx = clientX - (rect?.left ?? 0);
    const sy = clientY - (rect?.top ?? 0);
    const c = cam.current;
    return { x: (sx - c.x) / c.zoom, y: (sy - c.y) / c.zoom };
  }, []);

  const handleShelve = useCallback(
    (id: string) => {
      const node = nodeById.get(id);
      const label = node ? (node.kind === 'session' ? node.session.title : node.schedule.name) : 'Item';
      if (selectedId === id) setSelectedId(null);
      if (board && onTakeOff && onPutOn && id.startsWith('session:')) {
        onTakeOff([id]);
        onUndo(`Shelved “${label}” from ${board.name}`, () => onPutOn([id]), 'archive');
        return;
      }
      const before = prefs;
      updatePrefs((p) => shelveNode(p, id));
      onUndo(`Shelved “${label}” from ${board ? board.name : 'working set'}`, () => updatePrefs(() => before), 'archive');
    },
    [nodeById, board, onTakeOff, onPutOn, prefs, updatePrefs, selectedId, onUndo],
  );

  const hopNextAction = useCallback(() => {
    if (graph.actionOrder.length === 0) return;
    const curPos = selectedId ? graph.actionOrder.indexOf(selectedId) : -1;
    const nextIdx = curPos >= 0 ? (curPos + 1) % graph.actionOrder.length : actionIdx % graph.actionOrder.length;
    const targetId = graph.actionOrder[nextIdx];
    setActionIdx(nextIdx + 1);
    setSelectedClusterId(null);
    setSelectedId(targetId);
    centerOn(targetId);
  }, [graph.actionOrder, selectedId, actionIdx, centerOn]);

  // Keyboard shortcuts while Working Set mode is active.
  keys.current = (e: KeyboardEvent) => {
    if (editingCluster) return false;
    const key = e.key.toLowerCase();
    const mod = chordMod(e);
    if (e.key === ' ') {
      e.preventDefault();
      if (!e.repeat) setSpaceHeld(true);
      return true;
    }
    if (e.key === 'Tab' && !mod && !e.altKey) {
      e.preventDefault();
      hopNextAction();
      return true;
    }
    if (e.key === 'Escape') {
      setSelectedId(null);
      setSelectedClusterId(null);
      return true;
    }
    if (e.shiftKey && !mod && (e.code === 'Digit1' || e.code === 'Digit0')) {
      e.preventDefault();
      if (e.code === 'Digit1') fitAll();
      else zoomAt((rootRef.current?.clientWidth ?? 0) / 2, (rootRef.current?.clientHeight ?? 0) / 2, 1);
      return true;
    }
    if ((mod && (key === '=' || key === '+')) || (!mod && (e.key === '=' || e.key === '+'))) {
      e.preventDefault();
      zoomBy(1.2);
      return true;
    }
    if ((mod && key === '-') || (!mod && e.key === '-')) {
      e.preventDefault();
      zoomBy(0.83);
      return true;
    }
    if (mod || e.altKey) return false;
    if (key === 'a') {
      setActionOnly((on) => !on);
      return true;
    }
    if ((e.key === 'Backspace' || e.key === 'Delete') && selectedId) {
      e.preventDefault();
      handleShelve(selectedId);
      return true;
    }
    if (e.key === 'Enter' && selectedId) {
      const node = nodeById.get(selectedId);
      if (node?.kind === 'session') {
        e.preventDefault();
        onOpen({ kind: 'ref', ref: 'session', key: node.key, x: 0, y: 0, title: node.session.title });
        return true;
      }
    }
    return false;
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

  /* ---- Pointer dragging (pan, node drag-to-group/pin/detach, cluster drag) ---- */

  const onCanvasPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button !== 0 && e.button !== 1) return;
    if ((e.target as HTMLElement).closest('button, input, .ca-ws-panel, .ca-ws-hud, .ca-ws-toolbar')) return;

    const cardEl = (e.target as HTMLElement).closest<HTMLElement>('[data-ws-node]');
    const clusterHeadEl = (e.target as HTMLElement).closest<HTMLElement>('[data-ws-cluster-head]');

    if (!spaceHeld && e.button === 0 && cardEl) {
      const id = cardEl.dataset.wsNode!;
      const b = graph.boxes.get(id);
      if (!b) return;
      e.currentTarget.setPointerCapture(e.pointerId);
      const cl = clusterOfNode.get(id);
      drag.current = { kind: 'node', id, sx: e.clientX, sy: e.clientY, ox: b.x, oy: b.y, clusterId: cl?.id ?? null, moved: false };
      return;
    }

    if (!spaceHeld && e.button === 0 && clusterHeadEl) {
      const cid = clusterHeadEl.dataset.wsClusterHead!;
      const cl = clusterById.get(cid);
      if (!cl) return;
      e.currentTarget.setPointerCapture(e.pointerId);
      drag.current = { kind: 'cluster', id: cid, sx: e.clientX, sy: e.clientY, nodeIds: cl.nodeIds, moved: false };
      return;
    }

    // Background click / drag pans
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { kind: 'pan', sx: e.clientX, sy: e.clientY, cam: { ...cam.current } };
  };

  const onCanvasPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    if (d.kind === 'pan') {
      cam.current = { ...d.cam, x: d.cam.x + (e.clientX - d.sx), y: d.cam.y + (e.clientY - d.sy) };
      applyCamera();
      return;
    }
    const dx = (e.clientX - d.sx) / cam.current.zoom;
    const dy = (e.clientY - d.sy) / cam.current.zoom;
    if (!d.moved && Math.hypot(e.clientX - d.sx, e.clientY - d.sy) < 5) return;
    d.moved = true;

    if (d.kind === 'node') {
      setDragDelta({ ids: new Set([d.id]), dx, dy });
      // Detect drop target (another node or another cluster) under pointer
      const pt = toWorld(e.clientX, e.clientY);
      let hit: { kind: 'node' | 'cluster'; id: string } | null = null;
      for (const n of graph.nodes) {
        if (n.id === d.id) continue;
        const b = graph.boxes.get(n.id);
        if (b && hitsBox(pt, b, 6)) {
          hit = { kind: 'node', id: n.id };
          break;
        }
      }
      if (!hit) {
        for (const cl of graph.clusters) {
          if (cl.id === d.clusterId) continue;
          if (hitsBox(pt, cl, 8)) {
            hit = { kind: 'cluster', id: cl.id };
            break;
          }
        }
      }
      setDropTarget(hit);
    } else if (d.kind === 'cluster') {
      setDragDelta({ ids: new Set(d.nodeIds), dx, dy });
    }
  };

  const onCanvasPointerUp = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    drag.current = null;
    if (!d) return;

    if (d.kind === 'pan') {
      if (Math.hypot(e.clientX - d.sx, e.clientY - d.sy) < 4) {
        setSelectedId(null);
        setSelectedClusterId(null);
      }
      return;
    }

    const dx = (e.clientX - d.sx) / cam.current.zoom;
    const dy = (e.clientY - d.sy) / cam.current.zoom;
    const target = dropTarget;
    setDragDelta(null);
    setDropTarget(null);

    if (!d.moved) {
      if (d.kind === 'node') {
        const node = nodeById.get(d.id);
        setSelectedClusterId(null);
        setSelectedId(d.id);
        if (node?.kind === 'schedule' && onShowSchedule) onShowSchedule(node.schedule.id);
      } else if (d.kind === 'cluster') {
        setSelectedId(null);
        setSelectedClusterId((cur) => (cur === d.id ? null : d.id));
      }
      return;
    }

    if (d.kind === 'node') {
      const before = prefs;
      if (target?.kind === 'node') {
        const targetCluster = clusterOfNode.get(target.id);
        updatePrefs((p) => groupNodes(p, d.id, target.id, targetCluster?.id));
        onUndo('Grouped into cluster', () => updatePrefs(() => before), 'subtask');
      } else if (target?.kind === 'cluster') {
        const anchor = clusterById.get(target.id)?.nodeIds[0];
        if (anchor) {
          updatePrefs((p) => groupNodes(p, d.id, anchor, target.id));
          onUndo(`Moved into ${clusterById.get(target.id)?.title ?? 'cluster'}`, () => updatePrefs(() => before), 'subtask');
        }
      } else {
        const nx = d.ox + dx;
        const ny = d.oy + dy;
        const ownCluster = d.clusterId ? clusterById.get(d.clusterId) : null;
        const draggedOutside = ownCluster && !hitsBox({ x: nx + 140, y: ny + 50 }, ownCluster, 36);
        if (draggedOutside) {
          updatePrefs((p) => detachNode(p, d.id, { x: nx, y: ny }));
          onUndo('Detached from cluster', () => updatePrefs(() => before), 'pin');
        } else {
          updatePrefs((p) => pinNodeAt(p, d.id, { x: nx, y: ny }));
        }
      }
    } else if (d.kind === 'cluster') {
      updatePrefs((p) => pinClusterMoved(p, d.nodeIds, graph.boxes, dx, dy));
    }
  };

  /* ---- Context menus & Pull-in menu ---- */

  const openNodeMenu = (e: React.MouseEvent, node: WorkingNode) => {
    e.preventDefault();
    e.stopPropagation();
    const extra: MenuSpec = [
      { label: 'Shelve from working set', run: () => handleShelve(node.id) },
      ...(clusterOfNode.has(node.id)
        ? [
            {
              label: 'Detach from cluster',
              run: () => {
                const b = graph.boxes.get(node.id);
                if (b) updatePrefs((p) => detachNode(p, node.id, { x: b.x + 60, y: b.y + 40 }));
              },
            },
          ]
        : []),
      ...(prefs.pinnedPos[node.id]
        ? [
            {
              label: 'Unpin position',
              run: () =>
                updatePrefs((p) => {
                  const pinnedPos = { ...p.pinnedPos };
                  delete pinnedPos[node.id];
                  return { ...p, pinnedPos };
                }),
            },
          ]
        : []),
    ];
    if (node.kind === 'session') {
      const base = menuFor({ kind: 'ref', ref: 'session', key: node.key, x: 0, y: 0, title: node.session.title });
      void showContextMenu([...extra, 'separator', ...base], { x: e.clientX, y: e.clientY }).catch((err: unknown) => onToast(String(err)));
    } else {
      const spec: MenuSpec = [
        ...(onShowSchedule ? [{ label: 'Open in Schedules', run: () => onShowSchedule(node.schedule.id) }] : []),
        ...extra,
      ];
      void showContextMenu(spec, { x: e.clientX, y: e.clientY }).catch((err: unknown) => onToast(String(err)));
    }
  };

  const openPullInMenu = (e: React.MouseEvent<HTMLButtonElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    const items: MenuSpec = [];
    const noop = () => {};
    const addSession = (id: string) => {
      if (board && onPutOn) onPutOn([id]);
      else updatePrefs((cur) => pullInNode(cur, id));
    };
    const shelvedHubs = prefs.shelved.filter((id) => id.startsWith('schedule:'));
    if (graph.folded.shelvedSessions.length > 0) {
      items.push({ label: `Shelved (${graph.folded.shelvedSessions.length})`, run: noop, enabled: false });
      for (const p of graph.folded.shelvedSessions.slice(0, 10)) {
        const id = refId('session', p.session.key);
        items.push({ label: p.session.title, run: () => addSession(id) });
      }
      items.push({
        label: 'Restore all shelved',
        run: () => {
          if (board && onPutOn) onPutOn(graph.folded.shelvedSessions.map((p) => refId('session', p.session.key)));
          else updatePrefs((cur) => ({ ...cur, shelved: cur.shelved.filter((id) => id.startsWith('schedule:')) }));
        },
      });
    }
    if (shelvedHubs.length > 0) {
      if (items.length > 0) items.push('separator');
      items.push({
        label: `Restore hidden schedules (${shelvedHubs.length})`,
        run: () => updatePrefs((cur) => ({ ...cur, shelved: cur.shelved.filter((id) => !id.startsWith('schedule:')) })),
      });
    }
    if (graph.folded.quietSessions.length > 0) {
      if (items.length > 0) items.push('separator');
      const heading = board && !takesAll(board.rule) ? `Other sessions (${graph.folded.quietSessions.length})` : `Recent quiet sessions (${graph.folded.quietSessions.length})`;
      items.push({ label: heading, run: noop, enabled: false });
      for (const p of graph.folded.quietSessions.slice(0, 14)) {
        const id = refId('session', p.session.key);
        items.push({ label: p.session.title, run: () => addSession(id) });
      }
    }
    if (items.length === 0) items.push({ label: 'All sessions are already on this map', run: noop, enabled: false });
    void showContextMenu(items, { x: r.left, y: r.top - 120 }).catch((err: unknown) => onToast(String(err)));
  };

  const openCanvasMenu = (e: React.MouseEvent<HTMLDivElement>) => {
    if ((e.target as HTMLElement).closest('[data-ws-node], .ca-ws-panel, .ca-ws-hud, .ca-ws-toolbar')) return;
    e.preventDefault();
    const spec: MenuSpec = [
      { label: 'Zoom to fit', run: fitAll },
      {
        label: 'Relax layout',
        enabled: Object.keys(prefs.pinnedPos).length > 0,
        run: () => {
          updatePrefs(relaxLayout);
          window.setTimeout(fitAll, 30);
        },
      },
      ...(onSettings ? ['separator' as const, { label: 'Board settings…', run: onSettings }] : []),
    ];
    void showContextMenu(spec, { x: e.clientX, y: e.clientY }).catch((err: unknown) => onToast(String(err)));
  };

  /* ---- Filtering / highlight computation ---- */

  const focusNodeId = hoveredId ?? selectedId;
  const connectedIds = useMemo(() => {
    if (!focusNodeId) return null;
    const set = new Set<string>([focusNodeId]);
    for (const e of graph.edges) {
      if (e.from === focusNodeId) set.add(e.to);
      if (e.to === focusNodeId) set.add(e.from);
    }
    return set;
  }, [focusNodeId, graph.edges]);

  const qNorm = query.trim().toLowerCase();
  const matchesFilter = useCallback(
    (n: WorkingNode) => {
      if (filtering && matches && n.kind === 'session') {
        return matches({ kind: 'ref', ref: 'session', key: n.key, x: 0, y: 0, title: n.session.title });
      }
      if (!qNorm) return true;
      const hay =
        n.kind === 'session'
          ? `${n.session.title} ${n.summary} ${n.lane.repo} ${n.lane.callsign} ${n.session.account}`.toLowerCase()
          : `${n.schedule.name} ${n.nextLabel}`.toLowerCase();
      return qNorm.split(/\s+/).every((w) => hay.includes(w));
    },
    [filtering, matches, qNorm],
  );

  const selectedCluster = selectedClusterId ? clusterById.get(selectedClusterId) ?? null : null;
  const selectedClusterNodes = useMemo(
    () => (selectedCluster ? selectedCluster.nodeIds.map((id) => nodeById.get(id)).filter((n): n is WorkingNode => !!n) : []),
    [selectedCluster, nodeById],
  );

  const pinnedCount = Object.keys(prefs.pinnedPos).length;
  const urgentTotal = graph.nodes.filter((n) => n.action.tier === 'urgent').length;
  const actionTotal = graph.actionOrder.length;

  return (
    <div
      ref={rootRef}
      className={cx('ca-ws', spaceHeld && 'is-panning', !!dragDelta && 'is-dragging')}
      tabIndex={-1}
      onPointerDown={onCanvasPointerDown}
      onPointerMove={onCanvasPointerMove}
      onPointerUp={onCanvasPointerUp}
      onPointerCancel={onCanvasPointerUp}
      onContextMenu={openCanvasMenu}
      aria-label="Working set map"
    >
      {/* Top-left floating HUD: Action hopper + Action spotlight */}
      <div className="ca-ws-hud" role="toolbar" aria-label="Working set controls">
        {actionTotal > 0 ? (
          <button
            type="button"
            className={cx('ca-ws-hop', urgentTotal > 0 ? 'is-urgent' : 'is-handoff')}
            title="Jump to next session needing action (Tab)"
            onClick={hopNextAction}
          >
            <span className="ca-ws-hop-dot" aria-hidden="true" />
            <span>{actionTotal} {actionTotal === 1 ? 'needs action' : 'need action'}</span>
            <kbd className="ca-ws-kbd">Tab</kbd>
          </button>
        ) : (
          <span className="ca-ws-clear">
            <Icon name="circle-check" size={13} />
            <span>All clear</span>
          </span>
        )}
        <button
          type="button"
          className={cx('ca-ws-pill', actionOnly && 'is-on')}
          aria-pressed={actionOnly}
          title="Dim sessions that don’t need action (A)"
          onClick={() => setActionOnly((v) => !v)}
        >
          <Icon name="filter" size={13} />
          <span>Action only</span>
        </button>
      </div>

      {/* World transform layer: clusters, curved edges, cards */}
      <div ref={worldRef} className="ca-ws-world">
        {/* Soft neighborhood cluster halos */}
        {graph.clusters.map((cl) => {
          const isMovingCluster = dragDelta && cl.nodeIds.every((id) => dragDelta.ids.has(id));
          const cxPos = cl.x + (isMovingCluster ? dragDelta.dx : 0);
          const cyPos = cl.y + (isMovingCluster ? dragDelta.dy : 0);
          const isDrop = dropTarget?.kind === 'cluster' && dropTarget.id === cl.id;
          return (
            <div
              key={cl.id}
              className={cx('ca-ws-cluster', `is-${cl.color}`, selectedClusterId === cl.id && 'is-selected', isDrop && 'is-drop')}
              style={{ transform: `translate(${cxPos}px, ${cyPos}px)`, width: cl.w, height: cl.h }}
            >
              <div
                className="ca-ws-cluster-head"
                data-ws-cluster-head={cl.id}
                onDoubleClick={(e) => {
                  e.stopPropagation();
                  setEditingCluster(cl.id);
                }}
                title="Click for cluster overview · Drag to move cluster · Double-click to rename"
              >
                <span className="ca-ws-cluster-dot" aria-hidden="true" />
                {editingCluster === cl.id ? (
                  <input
                    className="ca-ws-cluster-input"
                    autoFocus
                    defaultValue={cl.title}
                    onFocus={(e) => e.currentTarget.select()}
                    onBlur={(e) => {
                      updatePrefs((p) => renameCluster(p, cl.id, e.currentTarget.value));
                      setEditingCluster(null);
                    }}
                    onKeyDown={(e) => {
                      e.stopPropagation();
                      if (e.key === 'Enter' || e.key === 'Escape') e.currentTarget.blur();
                    }}
                    aria-label="Cluster title"
                    spellCheck={false}
                  />
                ) : (
                  <span className="ca-ws-cluster-title">{cl.title}</span>
                )}
                <span className="ca-ws-cluster-count">{cl.nodeIds.length}</span>
                {cl.actionCount > 0 && (
                  <span className={cx('ca-ws-cluster-badge', cl.urgentCount > 0 ? 'is-urgent' : 'is-handoff')}>
                    {cl.actionCount} {cl.actionCount === 1 ? 'action' : 'actions'}
                  </span>
                )}
              </div>
            </div>
          );
        })}

        {/* Relationship edges (SVG inside world space) */}
        <svg className="ca-ws-edges" aria-hidden="true">
          {graph.edges.map((e) => {
            const fromBox = liveBoxes.get(e.from);
            const toBox = liveBoxes.get(e.to);
            if (!fromBox || !toBox) return null;
            const curve = connector(fromBox, toBox);
            const active = !!focusNodeId && (e.from === focusNodeId || e.to === focusNodeId);
            const directed = e.kind === 'helper' || e.kind === 'origin' || e.kind === 'schedule';
            const [b1, b2] = directed ? arrowhead(curve, 7, 0.45) : [null, null];
            const mid = { x: (boxCenter(fromBox).x + boxCenter(toBox).x) / 2, y: (boxCenter(fromBox).y + boxCenter(toBox).y) / 2 };
            return (
              <g key={e.id} className={edgeStrokeClass(e, active)}>
                <path d={pathOf(curve)} />
                {directed && b1 && b2 && <path className="ca-ws-arrow" d={`M ${b1.x.toFixed(1)} ${b1.y.toFixed(1)} L ${curve.b.x.toFixed(1)} ${curve.b.y.toFixed(1)} L ${b2.x.toFixed(1)} ${b2.y.toFixed(1)}`} />}
                {active && e.label && (
                  <g transform={`translate(${mid.x.toFixed(1)}, ${mid.y.toFixed(1)})`}>
                    <rect className="ca-ws-edge-pill" x={-Math.min(54, e.label.length * 3.8 + 8)} y={-9} width={Math.min(108, e.label.length * 7.6 + 16)} height={18} rx={9} />
                    <text className="ca-ws-edge-text" textAnchor="middle" dominantBaseline="central">
                      {e.label.slice(0, 16)}
                    </text>
                  </g>
                )}
              </g>
            );
          })}
        </svg>

        {/* Session Cards & Schedule Hubs */}
        {graph.nodes.map((node) => {
          const box = liveBoxes.get(node.id);
          if (!box) return null;
          const isSelected = selectedId === node.id;
          const isDrop = dropTarget?.kind === 'node' && dropTarget.id === node.id;
          const isDraggingThis = !!dragDelta?.ids.has(node.id);
          const dimmed =
            (actionOnly && node.action.rank > 2) ||
            !matchesFilter(node) ||
            (connectedIds !== null && !connectedIds.has(node.id) && !isSelected);

          if (node.kind === 'schedule') {
            return (
              <div
                key={node.id}
                data-ws-node={node.id}
                className={cx('ca-ws-hub', `tier-${node.action.tier}`, isSelected && 'is-selected', isDrop && 'is-drop', dimmed && 'is-dimmed', isDraggingThis && 'is-moving')}
                style={{ transform: `translate(${box.x}px, ${box.y}px)`, width: box.w, height: box.h }}
                onPointerEnter={() => setHoveredId(node.id)}
                onPointerLeave={() => setHoveredId((h) => (h === node.id ? null : h))}
                onContextMenu={(e) => openNodeMenu(e, node)}
              >
                <div className="ca-ws-hub-top">
                  <Icon name={node.schedule.when.kind === 'watch' ? 'eye' : node.schedule.when.kind === 'time' ? 'clock' : 'bolt'} size={14} />
                  <span className="ca-ws-hub-title">{node.schedule.name}</span>
                  {node.quietRuns > 0 && <span className="ca-ws-chip is-muted">{node.quietRuns} folded</span>}
                  {node.pinned && <Icon name="pin" size={11} className="ca-ws-pin-mark" />}
                </div>
                <div className="ca-ws-hub-meta">
                  <span className="ca-ws-hub-next">{node.nextLabel}</span>
                  <span className="ca-ws-hub-when">{node.schedule.whenWords}</span>
                </div>
              </div>
            );
          }

          const s = node.session;
          return (
            <div
              key={node.id}
              data-ws-node={node.id}
              className={cx('ca-ws-card', `tier-${node.action.tier}`, isSelected && 'is-selected', isDrop && 'is-drop', dimmed && 'is-dimmed', isDraggingThis && 'is-moving')}
              style={{ transform: `translate(${box.x}px, ${box.y}px)`, width: box.w, height: box.h }}
              onPointerEnter={() => setHoveredId(node.id)}
              onPointerLeave={() => setHoveredId((h) => (h === node.id ? null : h))}
              onDoubleClick={(e) => {
                e.stopPropagation();
                onOpen({ kind: 'ref', ref: 'session', key: s.key, x: 0, y: 0, title: s.title });
              }}
              onContextMenu={(e) => openNodeMenu(e, node)}
            >
              <div className="ca-ws-card-top">
                <span className={cx('ca-ws-beacon', `tier-${node.action.tier}`)}>{node.action.label}</span>
                {node.action.deadline && <span className="ca-ws-deadline">{node.action.deadline}</span>}
                {node.scheduleName && <span className="ca-ws-sched-tag" title={`From schedule: ${node.scheduleName}`}><Icon name="clock" size={10} /></span>}
                <span className="ca-ws-age">
                  {s.status === 'working' && s.statusSince ? duration(s.statusSince, now) : ago(s.lastActivity, now)}
                </span>
                {node.pinned && <Icon name="pin" size={11} className="ca-ws-pin-mark" />}
                <button
                  type="button"
                  className="ca-ws-shelve-btn"
                  title="Shelve from working set (⌫)"
                  aria-label={`Shelve ${s.title}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    handleShelve(node.id);
                  }}
                >
                  <Icon name="x" size={12} />
                </button>
              </div>

              <div className="ca-ws-card-head">
                <SessionGlyph s={s} size={13} />
                <SessionEmblem emblem={s.emblem} of={s.key} size={15} />
                <span className="ca-ws-card-title">{s.title}</span>
              </div>

              <div className="ca-ws-card-summary">{node.summary}</div>

              <div className="ca-ws-card-foot">
                <span className="ca-ws-chip">{node.lane.repo || node.lane.callsign}</span>
                {node.prs.slice(0, 1).map((pr) => (
                  <PrChip key={pr.url} pr={pr} compact />
                ))}
                {node.timers.slice(0, 1).map((t) => (
                  <span key={t.scheduleId} className="ca-ws-timer" title={`${t.name}${node.timers.length > 1 ? ` (+${node.timers.length - 1} more)` : ''}`}>
                    <Icon name="alarm" size={11} />
                    {t.words}
                    {node.timers.length > 1 ? ` +${node.timers.length - 1}` : ''}
                  </span>
                ))}
                {node.helpers.length > 0 && (
                  <span className="ca-ws-chip is-muted" title={node.helpers.map((h) => h.session.title).join(', ')}>
                    +{node.helpers.length}
                  </span>
                )}
                <span className="ca-ws-account">{s.account}</span>
              </div>
            </div>
          );
        })}
      </div>

      {graph.nodes.length === 0 && (
        <div className="ca-ws-empty">
          <Icon name="sparkles" size={24} />
          <p>{board ? `No sessions on “${board.name}” right now.` : `Your working set is clear in ${project}.`}</p>
          <button type="button" className="ca-ws-pill" onClick={openPullInMenu}>
            <Icon name="plus" size={13} />
            <span>Pull in a session</span>
          </button>
        </div>
      )}

      {selectedCluster && (
        <WorkingSetClusterPanel
          cluster={selectedCluster}
          nodes={selectedClusterNodes}
          now={now}
          onSelectSession={(key) => {
            setSelectedClusterId(null);
            setSelectedId(`session:${key}`);
            centerOn(`session:${key}`);
          }}
          onOpenSession={(key) => onOpen({ kind: 'ref', ref: 'session', key, x: 0, y: 0 })}
          onShelve={handleShelve}
          onRename={(cid, title) => updatePrefs((p) => renameCluster(p, cid, title))}
          onClose={() => setSelectedClusterId(null)}
        />
      )}

      {/* Bottom floating toolbar */}
      <div className="ca-ws-toolbar" role="toolbar" aria-label="Working set tools" onDoubleClick={(e) => e.stopPropagation()}>
        <button
          type="button"
          className="ca-ws-tool is-wide"
          title="Pull shelved or quiet sessions onto the working set"
          onClick={openPullInMenu}
        >
          <Icon name="plus" size={14} />
          <span>Pull in</span>
          {(graph.folded.shelvedSessions.length > 0 || graph.folded.quietSessions.length > 0) && (
            <span className="ca-ws-tool-count">
              {graph.folded.shelvedSessions.length + graph.folded.quietSessions.length}
            </span>
          )}
        </button>
        <button
          type="button"
          className="ca-ws-tool is-wide"
          disabled={pinnedCount === 0}
          title="Clear pinned positions and re-settle the force layout"
          onClick={() => {
            updatePrefs(relaxLayout);
            window.setTimeout(fitAll, 30);
          }}
        >
          <Icon name="refresh" size={14} />
          <span>Relax{pinnedCount > 0 ? ` (${pinnedCount})` : ''}</span>
        </button>
        {(graph.folded.collapsedRuns > 0 || graph.folded.foldedHelpers > 0) && (
          <span className="ca-ws-folded-note">
            {[
              graph.folded.collapsedRuns > 0 && `${graph.folded.collapsedRuns} old runs folded`,
              graph.folded.foldedHelpers > 0 && `${graph.folded.foldedHelpers} helpers tucked`,
            ]
              .filter(Boolean)
              .join(' · ')}
          </span>
        )}
        <span className="ca-ws-sep" />
        <button type="button" className="ca-ws-tool" title="Zoom out (−)" aria-label="Zoom out" onClick={() => zoomBy(0.83)}>
          <Icon name="zoom-out" size={15} />
        </button>
        <button ref={zoomLabelRef} type="button" className="ca-ws-zoom" title={keysIn('Zoom to fit (⇧1). ⇧0 for 100%.')} onClick={fitAll}>
          100%
        </button>
        <button type="button" className="ca-ws-tool" title="Zoom in (+)" aria-label="Zoom in" onClick={() => zoomBy(1.2)}>
          <Icon name="zoom-in" size={15} />
        </button>
      </div>
    </div>
  );
}
